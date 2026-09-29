mod classifier;
mod query_executor;
pub mod structured;

use classifier::{classify, Intent, ProjectFilter};
use rusqlite::{Connection, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Answer {
    Metric {
        summary: String,
        value: i64,
    },
    ProjectList {
        summary: String,
        total: i64,
        projects: Vec<ProjectResult>,
    },
    GroupedSummary {
        summary: String,
        groups: Vec<GroupResult>,
    },
    TextSummary {
        summary: String,
        items: Vec<String>,
    },
    UnsupportedQuestion {
        summary: String,
        suggestions: Vec<&'static str>,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectResult {
    pub id: i64,
    pub project_name: String,
    pub mcr_id: String,
    pub project_status: String,
    pub risk_level: String,
    pub risk_score: i64,
    pub primary_risk_driver: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GroupResult {
    pub label: String,
    pub count: i64,
}

pub fn dataset_version(connection: &Connection) -> Result<String> {
    let (project_list, mcr): (i64, i64) = connection.query_row(
        "SELECT (SELECT COALESCE(MAX(id), 0) FROM import_history WHERE status = 'success'), \
                (SELECT COALESCE(MAX(id), 0) FROM mcr_sync_history)",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(format!("project-list:{project_list}:mcr:{mcr}"))
}

pub fn answer(connection: &Connection, question: &str) -> Result<Answer> {
    let intent = classify(question);
    match intent {
        Intent::TotalProjects => {
            let value = query_executor::count_projects(connection)?;
            Ok(Answer::Metric {
                summary: format!("The current dataset contains {value} projects."),
                value,
            })
        }
        Intent::RiskCount(level) => {
            let value = query_executor::count_risk_level(connection, level)?;
            Ok(Answer::Metric {
                summary: format!("{value} projects are {level} risk."),
                value,
            })
        }
        Intent::Projects(filter) => {
            let (total, projects) = query_executor::projects(connection, &filter)?;
            let topic = match &filter {
                ProjectFilter::Risk(level) => format!("{level} risk"),
                ProjectFilter::Overdue(Some(stage)) => format!("overdue {stage}"),
                ProjectFilter::Overdue(None) => "overdue milestones".into(),
                ProjectFilter::HighestRisk => "the highest risk scores".into(),
                ProjectFilter::Hold => "PRJ Status HOLD".into(),
                ProjectFilter::Customer(customer) => format!("customer {customer}"),
                ProjectFilter::Manager(manager) => format!("project manager {manager}"),
                ProjectFilter::CustomerRisk(customer, risk) => {
                    format!("customer {customer} and {risk} risk")
                }
                ProjectFilter::ManagerRisk(manager, risk) => {
                    format!("project manager {manager} and {risk} risk")
                }
                ProjectFilter::McrId(mcr_id) => format!("MCR-ID {mcr_id}"),
                ProjectFilter::Ecu => "ECU components".into(),
                ProjectFilter::NewComponents(Some(component)) => {
                    format!("newly released {component} components")
                }
                ProjectFilter::NewComponents(None) => "newly released components".into(),
            };
            let shown = projects.len() as i64;
            let summary = if matches!(filter, ProjectFilter::HighestRisk) {
                format!("Showing the {shown} projects with the highest risk scores.")
            } else if shown < total {
                format!("{total} projects match {topic}. Showing the first {shown}, ordered by risk score.")
            } else {
                format!("{total} projects match {topic}.")
            };
            Ok(Answer::ProjectList {
                summary,
                total,
                projects,
            })
        }
        Intent::HighRiskByCustomer => {
            let groups = query_executor::group_high_risk_customers(connection)?;
            Ok(Answer::GroupedSummary {
                summary: "Customers ranked by their number of High or Critical risk projects."
                    .into(),
                groups,
            })
        }
        Intent::CriticalByManager => {
            let groups = query_executor::group_critical_managers(connection)?;
            Ok(Answer::GroupedSummary {
                summary: "Project managers ranked by their number of Critical projects.".into(),
                groups,
            })
        }
        Intent::MainRiskDrivers => {
            let groups = query_executor::main_risk_drivers(connection)?;
            let items = groups
                .into_iter()
                .map(|group| format!("{} — {} projects", group.label, group.count))
                .collect();
            Ok(Answer::TextSummary {
                summary: "Most common primary risk drivers in the current dataset.".into(),
                items,
            })
        }
        Intent::Unsupported => Ok(Answer::UnsupportedQuestion {
            summary: "I can't answer this question with the current local query engine yet.".into(),
            suggestions: vec![
                "How many projects are Critical?",
                "Which projects are overdue?",
                "Show projects for customer X.",
            ],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    fn database() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        connection.execute_batch(r#"
            INSERT INTO projects (id,mcr_id,project_name,customer,project_manager,project_status,has_ecu,source_file,source_row)
            VALUES (1,'MCR-1','Alpha','Bosch','Alice','HOLD',1,'test.xlsx',2),
                   (2,'MCR-2','Beta','Bosch','Bob','WORK',0,'test.xlsx',3),
                   (3,'MCR-3','Gamma','Other','Alice','WORK',0,'test.xlsx',4);
            INSERT INTO project_risks (project_id,risk_score,risk_level,primary_risk_driver)
            VALUES (1,90,'Critical','Overdue SE3'), (2,70,'High','Supplier delay'), (3,10,'Low',NULL);
            INSERT INTO project_milestones (project_id,milestone_type,planned_date,actual_date,plan_source,actual_source)
            VALUES (1,'SE3',date('now','localtime','-1 day'),NULL,'MCR','Project List'),
                   (2,'SE4',date('now','localtime','-2 day'),NULL,'MCR','Project List'),
                   (3,'SE3',date('now','localtime','-2 day'),date('now','localtime','-2 day'),'Project List','Project List');
            INSERT INTO project_components (project_id,component_name,component_type)
            VALUES (1,'ECU','existing'), (2,'Rail','new_released');
        "#).unwrap();
        connection
    }

    fn ids(connection: &Connection, question: &str) -> Vec<i64> {
        match answer(connection, question).unwrap() {
            Answer::ProjectList { projects, .. } => {
                projects.into_iter().map(|project| project.id).collect()
            }
            _ => panic!("expected project list for {question}"),
        }
    }

    #[test]
    fn counts_current_committed_projects_and_risk_levels() {
        let connection = database();
        assert!(matches!(
            answer(&connection, "How many projects are there?").unwrap(),
            Answer::Metric { value: 3, .. }
        ));
        assert!(matches!(
            answer(&connection, "How many projects are Critical?").unwrap(),
            Answer::Metric { value: 1, .. }
        ));
        assert_eq!(ids(&connection, "Which projects are Critical?"), vec![1]);
    }

    #[test]
    fn dataset_version_changes_only_after_committed_import_or_mcr_sync() {
        let connection = database();
        assert_eq!(
            dataset_version(&connection).unwrap(),
            "project-list:0:mcr:0"
        );
        connection
            .execute_batch(
                "INSERT INTO import_history (file_name, status) VALUES ('failed.xlsx', 'failed');",
            )
            .unwrap();
        assert_eq!(
            dataset_version(&connection).unwrap(),
            "project-list:0:mcr:0"
        );
        connection
            .execute_batch(
                "INSERT INTO import_history (file_name, status) VALUES ('valid.xlsx', 'success');",
            )
            .unwrap();
        assert_eq!(
            dataset_version(&connection).unwrap(),
            "project-list:2:mcr:0"
        );
        connection.execute_batch("INSERT INTO mcr_sync_history (file_name, file_size, synced_at, master_ids, matched_projects, updated_milestones, projects_affected, warning_count) VALUES ('mcr.xlsx', 1, '2026-01-01', 1, 1, 1, 1, 0);").unwrap();
        assert_eq!(
            dataset_version(&connection).unwrap(),
            "project-list:2:mcr:1"
        );
    }

    #[test]
    fn overdue_queries_use_committed_milestone_dates_and_stage() {
        let connection = database();
        assert_eq!(ids(&connection, "Which projects are overdue?"), vec![1, 2]);
        assert_eq!(
            ids(&connection, "Which projects have overdue SE3?"),
            vec![1]
        );
        assert_eq!(
            ids(&connection, "Which projects have overdue SE4?"),
            vec![2]
        );
    }

    #[test]
    fn owner_status_and_component_queries_are_parameterized() {
        let connection = database();
        assert_eq!(
            ids(&connection, "Show projects for customer Bosch."),
            vec![1, 2]
        );
        assert_eq!(
            ids(&connection, "Show projects managed by Alice."),
            vec![1, 3]
        );
        assert_eq!(ids(&connection, "Which projects are on HOLD?"), vec![1]);
        assert_eq!(ids(&connection, "Which projects contain ECU?"), vec![1]);
        assert_eq!(
            ids(
                &connection,
                "Which projects have newly released Rail components?"
            ),
            vec![2]
        );
        assert!(ids(&connection, "Show projects for customer Bosch' OR 1=1 --.").is_empty());
    }

    #[test]
    fn normalized_bilingual_queries_use_the_existing_sql_layer() {
        let connection = database();
        assert_eq!(ids(&connection, "Which projects are Critical"), vec![1]);
        assert_eq!(ids(&connection, "Which projects are overdue"), vec![1, 2]);
        assert_eq!(ids(&connection, "Which projects have overdue SE3"), vec![1]);
        assert_eq!(ids(&connection, "Which projects are on HOLD"), vec![1]);
        assert_eq!(ids(&connection, "Which projects contain ECU"), vec![1]);
        assert_eq!(
            ids(&connection, "Show High Risk projects for customer Bosch"),
            vec![2]
        );
        assert_eq!(
            ids(&connection, "Show Critical projects managed by Alice"),
            vec![1]
        );
        assert_eq!(ids(&connection, "Show project with MCR-ID MCR-1"), vec![1]);
        assert!(ids(&connection, "Show Critical projects managed by Bob").is_empty());
    }

    #[test]
    fn summaries_and_unknown_questions_return_structured_answers() {
        let connection = database();
        assert!(
            matches!(answer(&connection, "Which customers have the most high-risk projects?").unwrap(), Answer::GroupedSummary { groups, .. } if groups[0].label == "Bosch" && groups[0].count == 2)
        );
        assert!(
            matches!(answer(&connection, "Which project managers own Critical projects?").unwrap(), Answer::GroupedSummary { groups, .. } if groups[0].label == "Alice")
        );
        assert!(
            matches!(answer(&connection, "What are the main risk drivers?").unwrap(), Answer::TextSummary { items, .. } if items.len() == 2)
        );
        assert!(matches!(
            answer(&connection, "Forecast next year's budget").unwrap(),
            Answer::UnsupportedQuestion { .. }
        ));
    }
}
