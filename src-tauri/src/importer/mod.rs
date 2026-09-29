pub mod mcr;
mod parser;
mod persistence;

use serde::Serialize;
use std::{fmt, path::Path};

#[cfg(test)]
pub use parser::parse_workbook;
pub use parser::prepare_workbook;
pub(crate) use parser::{COMPONENT_SEPARATORS, MILESTONES};
pub use persistence::replace_dataset;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFailure {
    pub code: String,
    pub message: String,
    pub details: Vec<String>,
}

impl ImportFailure {
    pub fn new(code: impl Into<String>, message: impl Into<String>, details: Vec<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details,
        }
    }
}

impl fmt::Display for ImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl std::error::Error for ImportFailure {}

#[derive(Debug, Clone)]
pub struct ParsedDataset {
    pub source_file: String,
    pub file_size: u64,
    pub projects: Vec<ParsedProject>,
}

#[derive(Debug, Clone)]
pub struct ParsedProject {
    pub project_no: Option<i64>,
    pub mcr_id: String,
    pub project_name: String,
    pub customer: Option<String>,
    pub p_id: Option<String>,
    pub acq_manager: Option<String>,
    pub project_manager: Option<String>,
    pub engine_segment: Option<String>,
    pub product_category: Option<String>,
    pub emission: Option<String>,
    pub apic: Option<String>,
    pub project_type: Option<String>,
    pub system: Option<String>,
    pub components: Option<String>,
    pub new_released_components: Option<String>,
    pub components_list: Vec<String>,
    pub new_released_components_list: Vec<String>,
    pub has_injector: bool,
    pub has_pump: bool,
    pub has_rail: bool,
    pub has_ecu: bool,
    pub has_dnox: bool,
    pub other_component: Option<String>,
    pub project_status: Option<String>,
    pub b_sample: Option<String>,
    pub c_sample: Option<String>,
    pub d_sample: Option<String>,
    pub source_row: i64,
    pub milestones: Vec<ParsedMilestone>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImportIssueSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportIssue {
    pub severity: ImportIssueSeverity,
    pub category: String,
    pub row: Option<i64>,
    pub field: Option<String>,
    pub original_value: Option<String>,
    pub normalized_value: Option<String>,
    pub issue: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub preview_token: String,
    pub file_name: String,
    pub projects_detected: usize,
    pub valid_records: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub duplicate_mcr_ids: usize,
    pub formula_errors: usize,
    pub normalized_dates: usize,
    pub normalized_component_lists: usize,
    pub issues: Vec<ImportIssue>,
}

#[derive(Debug, Clone)]
pub struct PreparedImport {
    pub dataset: ParsedDataset,
    pub preview: ImportPreview,
}

#[cfg(test)]
impl ImportPreview {
    pub fn validation_passed(&self) -> bool {
        self.error_count == 0
    }
}

pub fn ensure_preview_replaceable(preview: &ImportPreview) -> Result<(), ImportFailure> {
    if preview.error_count > 0 {
        return Err(ImportFailure::new(
            "validation_blocked",
            "Replacement was blocked because the preview contains validation errors.",
            vec![format!("{} blocking errors remain.", preview.error_count)],
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ParsedMilestone {
    pub milestone_type: &'static str,
    pub planned_date: Option<String>,
    pub actual_date: Option<String>,
    pub plan_source: String,
    pub actual_source: String,
}

pub fn validate_file(path: &Path) -> Result<(), ImportFailure> {
    if !path.is_file() {
        return Err(ImportFailure::new(
            "file_not_found",
            "The selected Excel file could not be found.",
            vec![path.display().to_string()],
        ));
    }

    let is_excel = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("xlsx") || extension.eq_ignore_ascii_case("xlsm")
        });

    if !is_excel {
        return Err(ImportFailure::new(
            "unsupported_file",
            "Unsupported file type. Select an .xlsx or .xlsm workbook.",
            Vec::new(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_preview_replaceable, parse_workbook, prepare_workbook, replace_dataset,
        ParsedDataset,
    };
    use crate::database::migrations;
    use rusqlite::Connection;
    use std::path::Path;

    #[test]
    fn accepts_macro_enabled_project_list_extension() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("Active_Project_List_Fake.xlsx");
        let temporary = std::env::temp_dir().join(format!(
            "project-risk-list-{}-{}.xlsm",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::copy(source, &temporary).expect("test workbook should be copied");
        let result = prepare_workbook(&temporary);
        std::fs::remove_file(&temporary).expect("temporary workbook should be removed");
        assert!(result.is_ok(), ".xlsm workbook should parse: {result:?}");
    }

    #[test]
    fn finds_project_list_after_cover_worksheet() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("project-list-second-sheet.xlsx");
        let result = prepare_workbook(&path).expect("later Project List worksheet should parse");
        assert_eq!(result.preview.projects_detected, 1);
        assert_eq!(result.dataset.projects[0].mcr_id, "MCR-TEST-1");
        assert_eq!(result.dataset.projects[0].source_row, 4);
    }

    #[test]
    fn imports_reference_workbook_into_sqlite() {
        let workbook_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("Active_Project_List_Fake.xlsx");
        assert!(
            workbook_path.is_file(),
            "reference workbook is missing at {}",
            workbook_path.display()
        );

        let dataset = parse_workbook(&workbook_path).expect("reference workbook should parse");
        let expected_projects = dataset.projects.len() as i64;
        let expected_milestones = dataset
            .projects
            .iter()
            .map(|project| project.milestones.len())
            .sum::<usize>() as i64;
        assert!(expected_projects > 0);
        assert_eq!(expected_milestones, expected_projects * 5);

        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        migrations::run(&mut connection).expect("database migration should succeed");
        let result = replace_dataset(&mut connection, &dataset, 0, 0)
            .expect("validated workbook should persist transactionally");

        assert_eq!(result.total_projects, expected_projects);
        assert_eq!(result.total_milestones, expected_milestones);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM projects", [], |row| row
                    .get::<_, i64>(0))
                .expect("project count should be readable"),
            expected_projects
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM project_milestones", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("milestone count should be readable"),
            expected_milestones
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM import_history", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("import history count should be readable"),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM project_risks", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("risk count should be readable"),
            expected_projects
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM ai_insights", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("insight count should be readable"),
            7
        );
        assert!(crate::risk_engine::cache_is_current(
            &connection,
            chrono::Local::now().date_naive()
        )
        .expect("a successful import should mark derived data current"));
        let dashboard = crate::repositories::dashboard::get(&connection)
            .expect("dashboard query should use imported data");
        assert_eq!(dashboard.total_projects, expected_projects);
        assert_eq!(dashboard.projects.len(), expected_projects.min(10) as usize);
        assert_eq!(
            dashboard.on_track_projects,
            connection
                .query_row(
                    r#"SELECT COUNT(*) FROM projects p
                    JOIN project_risks r ON r.project_id = p.id
                    WHERE r.risk_level IN ('Low', 'Medium')
                      AND NOT EXISTS (
                        SELECT 1 FROM project_milestones m
                        WHERE m.project_id = p.id
                          AND DATE(m.planned_date) < DATE('now', 'localtime')
                          AND m.actual_date IS NULL
                      )"#,
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("on-track count should be readable")
        );
        assert_eq!(
            dashboard.critical_projects,
            connection
                .query_row(
                    "SELECT COUNT(*) FROM project_risks WHERE risk_level = 'Critical'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("critical count should be readable")
        );
        assert_eq!(
            dashboard.overdue_projects,
            connection
                .query_row(
                    r#"SELECT COUNT(*) FROM projects p
                    WHERE EXISTS (
                      SELECT 1 FROM project_milestones m
                      WHERE m.project_id = p.id
                        AND DATE(m.planned_date) < DATE('now', 'localtime')
                        AND m.actual_date IS NULL
                    )"#,
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("overdue count should be readable")
        );

        let project_page = crate::repositories::projects::list(
            &connection,
            &crate::models::ProjectListQuery {
                limit: Some(20),
                ..Default::default()
            },
        )
        .expect("project list should query imported data");
        assert_eq!(project_page.total, expected_projects);
        assert_eq!(project_page.items.len(), expected_projects.min(20) as usize);

        let first_project = &dataset.projects[0];
        let search_page = crate::repositories::projects::list(
            &connection,
            &crate::models::ProjectListQuery {
                search: Some(first_project.mcr_id.clone()),
                limit: Some(20),
                ..Default::default()
            },
        )
        .expect("project search should query imported data");
        assert!(search_page
            .items
            .iter()
            .any(|project| project.mcr_id == first_project.mcr_id));

        let project_id = search_page
            .items
            .iter()
            .find(|project| project.mcr_id == first_project.mcr_id)
            .expect("searched project should exist")
            .id;
        let detail = crate::repositories::projects::detail(&connection, project_id)
            .expect("project detail should query all imported sections");
        assert_eq!(detail.mcr_id, first_project.mcr_id);
        assert_eq!(detail.milestones.len(), 5);
        assert!((0..=100).contains(&detail.risk.risk_score));
        assert!(!detail.source_file.is_empty());

        let risk_overview = crate::repositories::risks::overview(
            &connection,
            &crate::models::RiskOverviewQuery {
                limit: Some(20),
                offset: Some(0),
                ..crate::models::RiskOverviewQuery::default()
            },
        )
        .expect("risk overview should aggregate imported data");
        assert_eq!(risk_overview.total_projects, expected_projects);
        assert_eq!(
            risk_overview.critical + risk_overview.high + risk_overview.medium + risk_overview.low,
            expected_projects
        );
        assert_eq!(
            risk_overview.projects.len(),
            expected_projects.min(20) as usize
        );
        assert!(!risk_overview.breakdowns.customers.is_empty());
        assert!(risk_overview
            .projects
            .windows(2)
            .all(|projects| projects[0].risk_score >= projects[1].risk_score));

        let insight_overview = crate::repositories::insights::overview(&connection)
            .expect("rule-based insight overview should use imported data");
        assert_eq!(insight_overview.summary.total_projects, expected_projects);
        assert_eq!(insight_overview.insights.len(), 7);
        assert_eq!(insight_overview.milestone_analysis.len(), 5);
        assert!(!insight_overview.attention_projects.is_empty());
        assert!(!insight_overview.risk_by_customer.is_empty());
        assert!(!insight_overview.risk_by_project_manager.is_empty());
    }

    fn reference_dataset() -> ParsedDataset {
        let workbook_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("Active_Project_List_Fake.xlsx");
        parse_workbook(&workbook_path).expect("reference workbook should parse")
    }

    fn migrated_connection() -> Connection {
        let mut connection = Connection::open_in_memory().expect("in-memory database should open");
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .expect("foreign keys should be enabled");
        migrations::run(&mut connection).expect("database migration should succeed");
        connection
    }

    fn seed_previous_dataset(connection: &mut Connection) -> (ParsedDataset, i64) {
        let mut dataset = reference_dataset();
        dataset.projects.truncate(3);
        let count = dataset.projects.len() as i64;
        replace_dataset(connection, &dataset, 0, 0).expect("seed dataset should persist");
        (dataset, count)
    }

    fn assert_previous_dataset_preserved(connection: &Connection, count: i64, first_mcr: &str) {
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM projects", [], |row| row
                    .get::<_, i64>(0))
                .expect("project count should remain readable"),
            count
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM projects WHERE mcr_id = ?1",
                    [first_mcr],
                    |row| row.get::<_, i64>(0),
                )
                .expect("previous project should remain readable"),
            1
        );
    }

    #[test]
    fn normalized_components_and_null_dates_are_persisted_without_reparsing() {
        let mut dataset = reference_dataset();
        dataset.projects.truncate(1);
        dataset.projects[0].components = Some("Injector, Pump, Rail".into());
        dataset.projects[0].components_list = vec!["Injector".into(), "Pump".into(), "Rail".into()];
        dataset.projects[0].new_released_components = Some("ECU".into());
        dataset.projects[0].new_released_components_list = vec!["ECU".into()];
        dataset.projects[0].milestones[0].planned_date = None;

        let mut connection = migrated_connection();
        replace_dataset(&mut connection, &dataset, 2, 1)
            .expect("normalized dataset should persist");

        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM project_components", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT planned_date FROM project_milestones WHERE milestone_type = 'SE2'",
                    [],
                    |row| row.get::<_, Option<String>>(0),
                )
                .unwrap(),
            None
        );
        assert_eq!(
            connection
                .query_row("SELECT warning_count FROM import_history", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

    #[test]
    fn duplicate_reaching_database_rolls_back_to_previous_dataset() {
        let mut connection = migrated_connection();
        let (mut candidate, previous_count) = seed_previous_dataset(&mut connection);
        let first_mcr = candidate.projects[0].mcr_id.clone();
        candidate.projects[1].mcr_id = first_mcr.clone();

        assert!(replace_dataset(&mut connection, &candidate, 0, 0).is_err());
        assert_previous_dataset_preserved(&connection, previous_count, &first_mcr);
    }

    #[test]
    fn insert_and_derived_data_failures_roll_back_atomically() {
        for (trigger_name, table) in [
            ("fail_project", "projects"),
            ("fail_milestone", "project_milestones"),
            ("fail_risk", "project_risks"),
        ] {
            let mut connection = migrated_connection();
            let (candidate, previous_count) = seed_previous_dataset(&mut connection);
            let first_mcr = candidate.projects[0].mcr_id.clone();
            connection
                .execute_batch(&format!(
                    "CREATE TEMP TRIGGER {trigger_name} BEFORE INSERT ON {table} BEGIN SELECT RAISE(ABORT, 'injected failure'); END;"
                ))
                .expect("failure trigger should be installed");

            assert!(replace_dataset(&mut connection, &candidate, 0, 0).is_err());
            assert_previous_dataset_preserved(&connection, previous_count, &first_mcr);
        }
    }

    #[test]
    fn preview_with_errors_is_rejected_before_persistence() {
        let workbook_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("Active_Project_List_Fake.xlsx");
        let mut prepared = prepare_workbook(&workbook_path).expect("preview should be prepared");
        prepared.preview.error_count = 1;
        assert_eq!(
            ensure_preview_replaceable(&prepared.preview)
                .expect_err("blocking errors must reject replacement")
                .code,
            "validation_blocked"
        );
    }
}
