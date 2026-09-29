use crate::{enterprise_llm::EnterpriseQuestion, qa, repositories::projects};
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RetrievalStatus {
    Sufficient,
    Partial,
    Insufficient,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalAssessment {
    pub status: RetrievalStatus,
    pub total_matches: usize,
    pub included_records: usize,
    pub missing_fields: Vec<String>,
    pub reason: Option<String>,
}

pub struct BuiltContext {
    pub value: Value,
    pub project_count: usize,
    pub byte_count: usize,
    pub assessment: RetrievalAssessment,
}

fn assessment(
    status: RetrievalStatus,
    total_matches: usize,
    included_records: usize,
    missing_fields: Vec<String>,
    reason: Option<&str>,
) -> RetrievalAssessment {
    RetrievalAssessment {
        status,
        total_matches,
        included_records,
        missing_fields,
        reason: reason.map(str::to_string),
    }
}

fn attach_assessment(mut value: Value, assessment: &RetrievalAssessment) -> Value {
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "retrievalAssessment".into(),
            serde_json::to_value(assessment).unwrap_or_else(|_| {
                json!({
                    "status":"insufficient",
                    "reason":"Could not describe retrieval coverage."
                })
            }),
        );
    }
    value
}

fn has_risk_evidence(projects: &[Value]) -> bool {
    projects.iter().any(|project| {
        project
            .get("primaryRiskDriver")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
            || project
                .get("riskReasons")
                .and_then(Value::as_array)
                .is_some_and(|values| !values.is_empty())
    })
}

fn asks_for_risk_explanation(question: &str) -> bool {
    let lower = question.to_lowercase();
    ["why", "reason", "risk", "为什么", "为何", "原因", "风险"]
        .iter()
        .any(|term| lower.contains(term))
}

fn short(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

fn project_matches(
    connection: &Connection,
    question: &EnterpriseQuestion,
) -> Result<Vec<i64>, String> {
    let current_version =
        qa::dataset_version(connection).map_err(|_| "Could not check the dataset version.")?;
    let mut matched = Vec::new();
    let lower = question.question.to_lowercase();
    let mut statement = connection
        .prepare("SELECT id, mcr_id, project_name FROM projects ORDER BY length(project_name) DESC")
        .map_err(|_| "Could not search current projects.")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|_| "Could not search current projects.")?;
    for row in rows {
        let (id, mcr, name) = row.map_err(|_| "Could not search current projects.")?;
        if lower.contains(&mcr.to_lowercase())
            || (name.chars().count() >= 5 && lower.contains(&name.to_lowercase()))
        {
            matched.push(id);
            if matched.len() == 3 {
                break;
            }
        }
    }
    if matched.is_empty() && question.dataset_version.as_deref() == Some(current_version.as_str()) {
        let reference =
            lower.contains("first") || lower.contains("第一个") || lower.contains("第 1 个");
        let followup = reference
            || lower.contains("this project")
            || lower.contains("that project")
            || lower.contains("这个项目")
            || lower.contains("该项目")
            || lower.contains("这些项目")
            || lower.contains("those projects");
        if followup {
            matched.extend(
                question
                    .last_project_ids
                    .iter()
                    .copied()
                    .take(if reference { 1 } else { 3 }),
            );
        }
    }
    matched.sort_unstable();
    matched.dedup();
    Ok(matched)
}

fn specific_project(connection: &Connection, id: i64) -> Result<Value, String> {
    let detail =
        projects::detail(connection, id).map_err(|_| "Could not load the referenced project.")?;
    Ok(json!({
        "projectName": short(&detail.project_name, 150),
        "mcrId": detail.mcr_id,
        "customer": detail.customer.as_deref().map(|v| short(v, 100)),
        "owner": detail.project_manager.as_deref().map(|v| short(v, 100)),
        "prjStatus": detail.project_status,
        "riskLevel": detail.risk.risk_level,
        "riskScore": detail.risk.risk_score,
        "primaryRiskDriver": detail.risk.primary_risk_driver.as_deref().map(|v| short(v, 160)),
        "riskReasons": detail.risk.risk_reasons.iter().take(4).map(|v| short(v, 160)).collect::<Vec<_>>(),
        "components": detail.components.as_deref().map(|v| short(v, 220)),
        "milestones": detail.milestones.iter().map(|m| json!({
            "stage": m.milestone_type,
            "plannedDate": m.planned_date,
            "actualDate": m.actual_date,
            "deltaDays": m.delta_days,
            "status": m.status
        })).collect::<Vec<_>>()
    }))
}

fn portfolio_summary(connection: &Connection) -> Result<Value, String> {
    connection.query_row(
        "SELECT (SELECT COUNT(*) FROM projects), \
         COALESCE(SUM(risk_level='Critical'),0), COALESCE(SUM(risk_level='High'),0), \
         COALESCE(SUM(risk_level='Medium'),0), COALESCE(SUM(risk_level='Low'),0), \
         (SELECT COUNT(DISTINCT project_id) FROM project_milestones WHERE planned_date < date('now','localtime') AND actual_date IS NULL), \
         (SELECT COUNT(*) FROM project_milestones WHERE actual_date IS NOT NULL AND planned_date IS NOT NULL AND actual_date > planned_date) \
         FROM project_risks",
        [],
        |row| Ok(json!({
            "totalProjects": row.get::<_, i64>(0)?, "critical": row.get::<_, i64>(1)?,
            "high": row.get::<_, i64>(2)?, "medium": row.get::<_, i64>(3)?,
            "low": row.get::<_, i64>(4)?, "overdueProjects": row.get::<_, i64>(5)?,
            "completedLateMilestones": row.get::<_, i64>(6)?
        })),
    ).map_err(|_| "Could not build the portfolio summary.".into())
}

fn top_projects(connection: &Connection) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(
            "SELECT p.project_name,p.mcr_id,p.customer,p.project_manager,p.project_status, \
         r.risk_level,r.risk_score,r.primary_risk_driver,r.risk_reasons \
         FROM projects p JOIN project_risks r ON r.project_id=p.id \
         ORDER BY r.risk_score DESC,p.mcr_id ASC LIMIT 8",
        )
        .map_err(|_| "Could not build top-risk context.")?;
    let rows = statement.query_map([], |row| {
        let reasons: String = row.get(8)?;
        let reasons: Vec<String> = serde_json::from_str(&reasons).unwrap_or_default();
        Ok(json!({
            "projectName": short(&row.get::<_, String>(0)?, 120),
            "mcrId": row.get::<_, String>(1)?,
            "customer": row.get::<_, Option<String>>(2)?.as_deref().map(|v| short(v, 90)),
            "owner": row.get::<_, Option<String>>(3)?.as_deref().map(|v| short(v, 90)),
            "prjStatus": row.get::<_, Option<String>>(4)?,
            "riskLevel": row.get::<_, String>(5)?,
            "riskScore": row.get::<_, i64>(6)?,
            "primaryRiskDriver": row.get::<_, Option<String>>(7)?.as_deref().map(|v| short(v, 150)),
            "riskReasons": reasons.iter().take(3).map(|v| short(v, 140)).collect::<Vec<_>>()
        }))
    }).map_err(|_| "Could not build top-risk context.")?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Could not build top-risk context.".into())
}

fn risk_drivers(connection: &Connection) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(
            "SELECT primary_risk_driver, COUNT(*) FROM project_risks \
         WHERE primary_risk_driver IS NOT NULL AND TRIM(primary_risk_driver) <> '' \
         GROUP BY primary_risk_driver ORDER BY COUNT(*) DESC, primary_risk_driver LIMIT 6",
        )
        .map_err(|_| "Could not build risk drivers.")?;
    let rows = statement.query_map([], |row| Ok(json!({"driver": short(&row.get::<_, String>(0)?, 140), "projects": row.get::<_, i64>(1)?})))
        .map_err(|_| "Could not build risk drivers.")?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Could not build risk drivers.".into())
}

fn customer_exposure(connection: &Connection) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(
            "SELECT COALESCE(NULLIF(TRIM(p.customer),''),'Unassigned'), COUNT(*), \
         SUM(r.risk_level IN ('High','Critical')), SUM(r.risk_score) \
         FROM projects p JOIN project_risks r ON r.project_id=p.id \
         GROUP BY 1 ORDER BY 4 DESC, 1 LIMIT 8",
        )
        .map_err(|_| "Could not build customer risk context.")?;
    let rows = statement.query_map([], |row| Ok(json!({
        "customer": short(&row.get::<_, String>(0)?, 100), "projects": row.get::<_, i64>(1)?,
        "highCritical": row.get::<_, i64>(2)?, "cumulativeRiskScore": row.get::<_, i64>(3)?
    }))).map_err(|_| "Could not build customer risk context.")?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Could not build customer risk context.".into())
}

fn project_manager_exposure(connection: &Connection) -> Result<Vec<Value>, String> {
    let items = crate::repositories::risks::breakdown(connection, "project_manager")
        .map_err(|_| "Could not build project-manager risk context.")?;
    serde_json::to_value(items)
        .map_err(|_| "Could not build project-manager risk context.".to_string())?
        .as_array()
        .cloned()
        .ok_or_else(|| "Could not build project-manager risk context.".into())
}

fn named_dimension(
    connection: &Connection,
    question: &str,
    column: &str,
) -> Result<Option<String>, String> {
    let sql = match column {
        "customer" => "SELECT DISTINCT customer FROM projects WHERE customer IS NOT NULL AND TRIM(customer)<>'' ORDER BY length(customer) DESC",
        "project_manager" => "SELECT DISTINCT project_manager FROM projects WHERE project_manager IS NOT NULL AND TRIM(project_manager)<>'' ORDER BY length(project_manager) DESC",
        _ => return Err("Unsupported project dimension.".into()),
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|_| "Could not search project groups.")?;
    let values = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| "Could not search project groups.")?;
    let lower = question.to_lowercase();
    for value in values {
        let value = value.map_err(|_| "Could not search project groups.")?;
        if value.chars().count() >= 3 && lower.contains(&value.to_lowercase()) {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn selected_dimension(connection: &Connection, column: &str, label: &str) -> Result<Value, String> {
    let column = match column {
        "customer" => "p.customer",
        "project_manager" => "p.project_manager",
        _ => return Err("Unsupported project dimension.".into()),
    };
    let aggregate_sql = format!("SELECT COUNT(*), COALESCE(SUM(r.risk_level='Critical'),0), COALESCE(SUM(r.risk_level='High'),0), COALESCE(SUM(r.risk_score),0) FROM projects p JOIN project_risks r ON r.project_id=p.id WHERE {column}=?1");
    let summary = connection
        .query_row(&aggregate_sql, [label], |row| {
            Ok(json!({
                "totalProjects":row.get::<_, i64>(0)?, "critical":row.get::<_, i64>(1)?,
                "high":row.get::<_, i64>(2)?, "cumulativeRiskScore":row.get::<_, i64>(3)?
            }))
        })
        .map_err(|_| "Could not summarize the selected project group.")?;
    let detail_sql = format!("SELECT p.project_name,p.mcr_id,p.project_status,r.risk_level,r.risk_score,r.primary_risk_driver FROM projects p JOIN project_risks r ON r.project_id=p.id WHERE {column}=?1 ORDER BY r.risk_score DESC,p.mcr_id LIMIT 6");
    let mut statement = connection
        .prepare(&detail_sql)
        .map_err(|_| "Could not load selected project summaries.")?;
    let rows = statement.query_map([label], |row| Ok(json!({
        "projectName":short(&row.get::<_, String>(0)?,120), "mcrId":row.get::<_, String>(1)?,
        "prjStatus":row.get::<_, Option<String>>(2)?, "riskLevel":row.get::<_, String>(3)?,
        "riskScore":row.get::<_, i64>(4)?, "primaryRiskDriver":row.get::<_, Option<String>>(5)?.as_deref().map(|v| short(v,150))
    }))).map_err(|_| "Could not load selected project summaries.")?;
    let projects = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Could not load selected project summaries.")?;
    Ok(
        json!({"scope":"namedProjectGroup", "dimension":column, "label":short(label,100), "summary":summary, "topRiskProjects":projects, "coverage":"Exact group counts; project examples limited to top 6"}),
    )
}

fn milestone_delays(connection: &Connection, stage: &str) -> Result<Vec<Value>, String> {
    let mut statement = connection.prepare(
        "SELECT p.project_name,p.mcr_id,m.milestone_type,m.planned_date,m.actual_date, \
         CAST(julianday(m.actual_date)-julianday(m.planned_date) AS INTEGER) AS delay_days \
         FROM project_milestones m JOIN projects p ON p.id=m.project_id \
         WHERE m.milestone_type=?1 AND m.actual_date IS NOT NULL AND m.planned_date IS NOT NULL AND m.actual_date>m.planned_date \
         ORDER BY delay_days DESC,p.mcr_id LIMIT 8"
    ).map_err(|_| "Could not build milestone delay context.")?;
    let rows = statement.query_map(params![stage], |row| Ok(json!({
        "projectName": short(&row.get::<_, String>(0)?, 120), "mcrId": row.get::<_, String>(1)?,
        "stage": row.get::<_, String>(2)?, "plannedDate": row.get::<_, String>(3)?,
        "actualDate": row.get::<_, String>(4)?, "delayDays": row.get::<_, i64>(5)?
    }))).map_err(|_| "Could not build milestone delay context.")?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "Could not build milestone delay context.".into())
}

pub fn build(
    connection: &Connection,
    question: &EnterpriseQuestion,
) -> Result<BuiltContext, String> {
    if let Some(query) = &question.structured_query {
        if question.requires_current_dataset {
            let current = qa::dataset_version(connection)
                .map_err(|_| "Could not check the dataset version.")?;
            if question.dataset_version.as_deref() != Some(current.as_str()) {
                return Err(
                    "The dataset changed; restate the project scope before continuing.".into(),
                );
            }
        }
        let (total, ids) = qa::structured::matching_project_ids(connection, query, 8)?;
        let total_matches = total.max(0) as usize;
        let projects = ids
            .iter()
            .map(|id| specific_project(connection, *id))
            .collect::<Result<Vec<_>, _>>()?;
        let mut retrieval = if total_matches == 0 {
            assessment(
                RetrievalStatus::Insufficient,
                0,
                0,
                vec![],
                Some("No projects matched the controlled SQLite query."),
            )
        } else if total_matches > ids.len() {
            assessment(
                RetrievalStatus::Partial,
                total_matches,
                ids.len(),
                vec![],
                Some("The exact match count is available, but project evidence is limited to the first 8 matching records."),
            )
        } else {
            assessment(
                RetrievalStatus::Sufficient,
                total_matches,
                ids.len(),
                vec![],
                None,
            )
        };
        if total_matches > 0
            && asks_for_risk_explanation(&question.question)
            && !has_risk_evidence(&projects)
        {
            retrieval.status = RetrievalStatus::Insufficient;
            retrieval.missing_fields = vec!["primaryRiskDriver".into(), "riskReasons".into()];
            retrieval.reason = Some("Matching projects were found, but the database contains no risk reasons or primary risk driver for them.".into());
        }
        let value = attach_assessment(
            json!({"scope":"structuredProjectSelection", "matchingProjects":total, "projects":projects, "coverage":format!("Exact matching count; {} of {} matching project records included.", ids.len(), total)}),
            &retrieval,
        );
        let byte_count = serde_json::to_vec(&value)
            .map_err(|_| "Could not serialize project context.")?
            .len();
        if byte_count > 16_000 {
            return Err("The selected project context is too large; narrow the question.".into());
        }
        return Ok(BuiltContext {
            value,
            project_count: ids.len(),
            byte_count,
            assessment: retrieval,
        });
    }
    let matched = project_matches(connection, question)?;
    let lower = question.question.to_lowercase();
    let named_customer = if lower.contains("customer") || lower.contains("客户") {
        named_dimension(connection, &lower, "customer")?
    } else {
        None
    };
    let named_manager = if lower.contains("manager")
        || lower.contains("owner")
        || lower.contains("经理")
        || lower.contains("负责人")
    {
        named_dimension(connection, &lower, "project_manager")?
    } else {
        None
    };
    let has_reference = [
        "this project",
        "that project",
        "first project",
        "这个项目",
        "该项目",
        "第一个项目",
    ]
    .iter()
    .any(|term| lower.contains(term));
    let mut value = if !matched.is_empty() {
        let projects = matched
            .iter()
            .map(|id| specific_project(connection, *id))
            .collect::<Result<Vec<_>, _>>()?;
        json!({"scope":"specificProjects", "projects":projects, "selection":"Exact name/MCR-ID or current-dataset follow-up reference"})
    } else if has_reference {
        json!({"scope":"insufficient", "reason":"The referenced project could not be identified in the current dataset."})
    } else if let Some(stage) = ["SE2", "SE3", "SE4", "QGC0", "QGC4"]
        .iter()
        .find(|stage| question.question.to_uppercase().contains(**stage))
    {
        let delays = milestone_delays(connection, stage)?;
        json!({"scope":"milestoneCompletedDelayRanking", "stage":stage, "rankingBasis":"actualDate minus plannedDate; completed late only", "topCompletedDelays":delays, "coverage":"Top 8 only; not a full milestone export"})
    } else if let Some(label) = named_customer {
        selected_dimension(connection, "customer", &label)?
    } else if let Some(label) = named_manager {
        selected_dimension(connection, "project_manager", &label)?
    } else if lower.contains("manager")
        || lower.contains("owner")
        || lower.contains("engineer")
        || lower.contains("经理")
        || lower.contains("负责人")
        || lower.contains("工程师")
    {
        json!({
            "scope":"projectManagerRiskExposure",
            "topProjectManagers":project_manager_exposure(connection)?,
            "rankingBasis":"sum of deterministic Risk Score across all projects owned by each Project Manager",
            "coverage":"Top 8 Project Managers by cumulative risk exposure"
        })
    } else if lower.contains("customer") || lower.contains("客户") {
        json!({"scope":"customerRiskExposure", "portfolioSummary":portfolio_summary(connection)?, "topCustomers":customer_exposure(connection)?, "rankingBasis":"sum of deterministic risk scores", "coverage":"Top 8 customers by exposure"})
    } else {
        let summary = portfolio_summary(connection)?;
        let projects = top_projects(connection)?;
        let drivers = risk_drivers(connection)?;
        let missing_status: i64 = connection.query_row("SELECT COUNT(*) FROM projects WHERE project_status IS NULL OR TRIM(project_status)=''", [], |row| row.get(0)).map_err(|_| "Could not build data-quality notes.")?;
        json!({"scope":"portfolioOverview", "portfolioSummary":summary, "topRiskProjects":projects, "majorRiskDrivers":drivers, "dataQualityNotes":[format!("{missing_status} projects have no PRJ Status")], "coverage":"Only top 8 risk projects; do not claim this is the complete project list"})
    };
    let project_count = value
        .get("projects")
        .or_else(|| value.get("topRiskProjects"))
        .or_else(|| value.get("topCompletedDelays"))
        .or_else(|| value.get("topCustomers"))
        .or_else(|| value.get("topProjectManagers"))
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let scope = value
        .get("scope")
        .and_then(Value::as_str)
        .unwrap_or("insufficient");
    let total_matches = value
        .pointer("/portfolioSummary/totalProjects")
        .or_else(|| value.pointer("/summary/totalProjects"))
        .and_then(Value::as_i64)
        .unwrap_or(project_count as i64)
        .max(0) as usize;
    let retrieval = match scope {
        "insufficient" => assessment(
            RetrievalStatus::Insufficient,
            0,
            0,
            vec![],
            value.get("reason").and_then(Value::as_str),
        ),
        "specificProjects" => {
            if project_count == 0 {
                assessment(RetrievalStatus::Insufficient, 0, 0, vec![], Some("No referenced project could be retrieved."))
            } else if asks_for_risk_explanation(&question.question)
                && !value
                    .get("projects")
                    .and_then(Value::as_array)
                    .is_some_and(|projects| has_risk_evidence(projects))
            {
                assessment(
                    RetrievalStatus::Insufficient,
                    project_count,
                    project_count,
                    vec!["primaryRiskDriver".into(), "riskReasons".into()],
                    Some("The referenced project was found, but it has no risk reasons or primary risk driver in SQLite."),
                )
            } else {
                assessment(RetrievalStatus::Sufficient, project_count, project_count, vec![], None)
            }
        }
        "namedProjectGroup" => assessment(
            if total_matches > project_count { RetrievalStatus::Partial } else { RetrievalStatus::Sufficient },
            total_matches,
            project_count,
            vec![],
            (total_matches > project_count).then_some("Exact group totals are available, but detailed evidence is limited to the top 6 projects."),
        ),
        "milestoneCompletedDelayRanking" => assessment(
            if project_count == 0 { RetrievalStatus::Insufficient } else { RetrievalStatus::Partial },
            project_count,
            project_count,
            vec![],
            Some(if project_count == 0 { "No completed late milestones matched the requested stage." } else { "The ranking is limited to the top 8 completed delays." }),
        ),
        "portfolioOverview" => assessment(
            if total_matches == 0 { RetrievalStatus::Insufficient } else if total_matches > project_count { RetrievalStatus::Partial } else { RetrievalStatus::Sufficient },
            total_matches,
            project_count,
            vec![],
            if total_matches == 0 { Some("The current SQLite dataset contains no projects.") } else if total_matches > project_count { Some("Portfolio totals are exact, but detailed evidence is limited to the top 8 risk projects.") } else { None },
        ),
        "customerRiskExposure" => assessment(
            if project_count == 0 { RetrievalStatus::Insufficient } else { RetrievalStatus::Partial },
            total_matches,
            project_count,
            vec![],
            Some("Customer exposure evidence is limited to the top 8 customer groups."),
        ),
        "projectManagerRiskExposure" => assessment(
            if project_count == 0 { RetrievalStatus::Insufficient } else { RetrievalStatus::Partial },
            project_count,
            project_count,
            vec![],
            Some(if project_count == 0 { "No Project Manager risk exposure data is available." } else { "Project Manager ranking is limited to the top 8 by cumulative risk exposure." }),
        ),
        _ => assessment(RetrievalStatus::Partial, total_matches, project_count, vec![], Some("Only bounded SQLite evidence was retrieved.")),
    };
    value = attach_assessment(value, &retrieval);
    let byte_count = serde_json::to_vec(&value)
        .map_err(|_| "Could not serialize project context.")?
        .len();
    if byte_count > 16_000 {
        return Err("The selected project context is too large; narrow the question.".into());
    }
    Ok(BuiltContext {
        value,
        project_count,
        byte_count,
        assessment: retrieval,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    fn fixture() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        connection.execute_batch(r#"
          INSERT INTO projects (id,mcr_id,project_name,customer,project_manager,project_status,components,source_file,source_row)
          VALUES (1,'BM-10001','Phoenix ERP','Bosch','Alice','WORK','ECU, Pump','secret.xlsx',2),
                 (2,'BM-10002','Orion Platform','Acme','Bob','HOLD','Rail','secret.xlsx',3),
                 (3,'BM-10003','Atlas Migration','Bosch','Alice','PLAN',NULL,'secret.xlsx',4);
          INSERT INTO project_risks (project_id,risk_score,risk_level,primary_risk_driver,risk_reasons)
          VALUES (1,82,'Critical','Overdue SE3','["Overdue SE3"]'),
                 (2,65,'High','Project status HOLD','["HOLD status"]'),
                 (3,10,'Low',NULL,'[]');
          INSERT INTO project_milestones (project_id,milestone_type,planned_date,actual_date,plan_source,actual_source)
          VALUES (1,'SE3','2025-01-01',NULL,'Project List','Project List'),
                 (2,'SE4','2025-01-01','2025-02-01','Project List','Project List');
        "#).unwrap();
        connection
    }

    fn question(text: &str) -> EnterpriseQuestion {
        EnterpriseQuestion {
            question: text.into(),
            language: "en".into(),
            structured_query: None,
            last_project_ids: vec![],
            dataset_version: None,
            requires_current_dataset: false,
            direct_conversation: false,
            recent_messages: vec![],
        }
    }

    #[test]
    fn broad_context_is_bounded_and_excludes_excel_columns() {
        let context = build(&fixture(), &question("Summarize the portfolio risks")).unwrap();
        assert_eq!(context.value["portfolioSummary"]["totalProjects"], 3);
        assert_eq!(context.project_count, 3);
        assert!(context.byte_count < 16_000);
        let serialized = context.value.to_string();
        assert!(!serialized.contains("secret.xlsx"));
        assert!(!serialized.contains("newReleasedComponents"));
    }

    #[test]
    fn broad_context_sends_top_eight_not_the_full_project_list() {
        let connection = fixture();
        for id in 4..24 {
            connection.execute("INSERT INTO projects (id,mcr_id,project_name,source_file,source_row) VALUES (?1,?2,?3,'secret.xlsx',?1)", params![id, format!("BM-{id:05}"), format!("Extra Project {id}")]).unwrap();
            connection.execute("INSERT INTO project_risks (project_id,risk_score,risk_level,risk_reasons) VALUES (?1,1,'Low','[]')", [id]).unwrap();
        }
        let context = build(&connection, &question("Summarize the portfolio risks")).unwrap();
        assert_eq!(context.value["portfolioSummary"]["totalProjects"], 23);
        assert_eq!(context.project_count, 8);
        assert_eq!(context.assessment.status, RetrievalStatus::Partial);
        assert_eq!(context.value["retrievalAssessment"]["status"], "partial");
        assert_eq!(context.value["retrievalAssessment"]["totalMatches"], 23);
        assert!(context.byte_count < 16_000);
    }

    #[test]
    fn structured_analysis_scope_uses_exact_sql_selection_not_global_top_risk_sample() {
        use crate::qa::structured::{Entity, Field, Filter, Operation, Operator, StructuredQuery};
        let mut request = question("Why are projects owned by Alice risky?");
        request.structured_query = Some(StructuredQuery {
            entity: Entity::Project,
            operation: Operation::List,
            target: None,
            filters: vec![Filter {
                field: Field::Owner,
                operator: Operator::Equals,
                value: Some("Alice".into()),
                values: None,
            }],
            sort: None,
            limit: Some(8),
        });
        let context = build(&fixture(), &request).unwrap();
        assert_eq!(context.value["scope"], "structuredProjectSelection");
        assert_eq!(context.value["matchingProjects"], 2);
        assert_eq!(context.project_count, 2);
        assert!(!context.value.to_string().contains("Orion Platform"));
        assert!(context.byte_count < 16_000);
    }

    #[test]
    fn structured_retrieval_reports_no_matches_and_missing_risk_evidence() {
        use crate::qa::structured::{Entity, Field, Filter, Operation, Operator, StructuredQuery};
        let connection = fixture();
        let mut request = question("Why are UNKNOWN status projects risky?");
        request.structured_query = Some(StructuredQuery {
            entity: Entity::Project,
            operation: Operation::List,
            target: None,
            filters: vec![Filter {
                field: Field::PrjStatus,
                operator: Operator::Equals,
                value: Some("UNKNOWN".into()),
                values: None,
            }],
            sort: None,
            limit: Some(8),
        });
        let no_match = build(&connection, &request).unwrap();
        assert_eq!(no_match.assessment.status, RetrievalStatus::Insufficient);
        assert_eq!(no_match.assessment.total_matches, 0);

        request.question = "Why is the PLAN project risky?".into();
        request.structured_query.as_mut().unwrap().filters[0].value = Some("PLAN".into());
        let missing_reasons = build(&connection, &request).unwrap();
        assert_eq!(
            missing_reasons.assessment.status,
            RetrievalStatus::Insufficient
        );
        assert!(missing_reasons
            .assessment
            .missing_fields
            .contains(&"riskReasons".into()));
        assert_eq!(
            missing_reasons.value["retrievalAssessment"]["status"],
            "insufficient"
        );
    }

    #[test]
    fn named_project_context_contains_only_matching_project_and_milestones() {
        let context = build(&fixture(), &question("Why is Phoenix ERP Critical?")).unwrap();
        assert_eq!(context.value["scope"], "specificProjects");
        assert_eq!(context.project_count, 1);
        assert_eq!(context.value["projects"][0]["mcrId"], "BM-10001");
        assert_eq!(
            context.value["projects"][0]["milestones"][0]["stage"],
            "SE3"
        );
        assert!(!context.value.to_string().contains("Orion Platform"));
    }

    #[test]
    fn followup_project_ids_require_current_dataset_version() {
        let connection = fixture();
        let mut q = question("Why is the first project so risky?");
        q.last_project_ids = vec![1, 2];
        q.dataset_version = Some(qa::dataset_version(&connection).unwrap());
        assert_eq!(
            build(&connection, &q).unwrap().value["projects"][0]["mcrId"],
            "BM-10001"
        );
        q.dataset_version = Some("old-dataset".into());
        assert_eq!(
            build(&connection, &q).unwrap().value["scope"],
            "insufficient"
        );
    }

    #[test]
    fn named_customer_context_uses_exact_group_counts() {
        let context = build(
            &fixture(),
            &question("How many projects does customer Bosch have?"),
        )
        .unwrap();
        assert_eq!(context.value["scope"], "namedProjectGroup");
        assert_eq!(context.value["summary"]["totalProjects"], 2);
        assert!(!context.value.to_string().contains("Orion Platform"));
    }

    #[test]
    fn stage_delay_context_uses_calculated_completed_delays() {
        let context = build(
            &fixture(),
            &question("Which project has the highest SE4 delay?"),
        )
        .unwrap();
        assert_eq!(context.value["scope"], "milestoneCompletedDelayRanking");
        assert_eq!(context.value["topCompletedDelays"][0]["mcrId"], "BM-10002");
        assert_eq!(context.value["topCompletedDelays"][0]["delayDays"], 31);
    }

    #[test]
    fn project_manager_risk_ranking_reuses_dashboard_exposure_rules() {
        let context = build(
            &fixture(),
            &question("Which project managers have the highest risk exposure?"),
        )
        .unwrap();
        assert_eq!(context.value["scope"], "projectManagerRiskExposure");
        assert_eq!(
            context.value["rankingBasis"],
            "sum of deterministic Risk Score across all projects owned by each Project Manager"
        );
        assert_eq!(context.value["topProjectManagers"][0]["label"], "Alice");
        assert_eq!(context.value["topProjectManagers"][0]["exposure"], 92);
        assert_eq!(context.assessment.status, RetrievalStatus::Partial);
    }
}
