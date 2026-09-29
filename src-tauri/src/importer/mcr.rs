//! MCR is a targeted planned-date source; it never creates projects or changes actual dates.
use super::{
    parser::{find_header_row, parse_mcr_date_cell, SourceCell},
    validate_file, ImportFailure, ImportIssue, ImportIssueSeverity,
};
use calamine::{open_workbook, Data, DataType, Reader, Xlsx, XlsxError};
use chrono::{Local, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
};

const REQUIRED_HEADERS: [&str; 4] = [
    "PRJ MCR ID K",
    "MCR ID Level 0",
    "WP: WPI-Nr ID T",
    "Date formate",
];
pub(crate) const STAGES: [&str; 5] = ["SE2", "SE3", "SE4", "QGC0", "QGC4"];
pub(crate) const EXCLUDED_STAGES: [&str; 1] = ["QG0"];

#[derive(Debug, Clone)]
struct McrRow {
    row: i64,
    level0: String,
    stage: &'static str,
    date: String,
}

#[derive(Debug, Clone)]
struct Aggregated {
    level0: String,
    stage: &'static str,
    latest_date: String,
    contributing_rows: Vec<i64>,
}

#[derive(Debug)]
struct ParsedMcr {
    file_name: String,
    file_size: u64,
    rows_detected: usize,
    master_ids: HashSet<String>,
    dates: Vec<Aggregated>,
    issues: Vec<ImportIssue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McrUpdateItem {
    pub project_id: i64,
    pub mcr_id: String,
    pub project_name: String,
    pub milestone_type: String,
    pub current_plan_date: Option<String>,
    pub current_plan_source: String,
    pub new_plan_date: String,
    pub contributing_rows: Vec<i64>,
    pub changed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McrPreview {
    pub preview_token: String,
    pub file_name: String,
    pub rows_detected: usize,
    pub master_ids_detected: usize,
    pub matched_projects: usize,
    pub unmatched_master_ids: usize,
    pub milestone_dates_detected: usize,
    pub projects_affected: usize,
    pub warning_count: usize,
    pub error_count: usize,
    pub items: Vec<McrUpdateItem>,
    pub issues: Vec<ImportIssue>,
}

#[derive(Debug, Clone)]
pub struct PreparedMcr {
    pub preview: McrPreview,
    file_size: u64,
    base_revision: i64,
    sync_revision: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McrApplyResult {
    pub file_name: String,
    pub updated_milestones: usize,
    pub projects_affected: usize,
    pub applied_at: String,
}

pub fn prepare(path: &Path, connection: &Connection) -> Result<PreparedMcr, ImportFailure> {
    let mut parsed = parse(path)?;
    let project_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
        .map_err(db_error)?;
    if project_count == 0 {
        return Err(ImportFailure::new(
            "no_base_dataset",
            "Import the Project List before previewing MCR updates.",
            vec![],
        ));
    }

    let base_revision = revision(connection, "import_history").map_err(db_error)?;
    let sync_revision = revision(connection, "mcr_sync_history").map_err(db_error)?;
    let mut matched = HashMap::<String, (i64, String)>::new();
    for level0 in &parsed.master_ids {
        let project: Option<(i64, String)> = connection
            .query_row(
                "SELECT id, project_name FROM projects WHERE mcr_id = ?1",
                [level0],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(db_error)?;
        if let Some(project) = project {
            matched.insert(level0.clone(), project);
        } else {
            parsed.issues.push(issue(ImportIssueSeverity::Warning, "unmatched_master_id", None,
                "MCR ID Level 0", Some(level0.clone()), None,
                "No matching PRJ MCR-ID exists in the current Project List; no project was created."));
        }
    }

    let mut items = Vec::new();
    for date in &parsed.dates {
        let Some((project_id, project_name)) = matched.get(&date.level0) else {
            continue;
        };
        let current: Option<(Option<String>, String)> = connection.query_row(
            "SELECT planned_date, plan_source FROM project_milestones WHERE project_id = ?1 AND milestone_type = ?2",
            params![project_id, date.stage], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(db_error)?;
        let Some((current_plan_date, current_plan_source)) = current else {
            parsed.issues.push(issue(ImportIssueSeverity::Error, "missing_milestone", None,
                "Milestone", Some(format!("{} / {}", date.level0, date.stage)), None,
                "Matched project is missing this milestone record; update cannot be applied safely."));
            continue;
        };
        let changed = current_plan_date.as_deref() != Some(date.latest_date.as_str())
            || current_plan_source != "MCR";
        items.push(McrUpdateItem {
            project_id: *project_id,
            mcr_id: date.level0.clone(),
            project_name: project_name.clone(),
            milestone_type: date.stage.to_string(),
            current_plan_date,
            current_plan_source,
            new_plan_date: date.latest_date.clone(),
            contributing_rows: date.contributing_rows.clone(),
            changed,
        });
    }
    items.sort_by(|a, b| {
        a.mcr_id
            .cmp(&b.mcr_id)
            .then(stage_order(&a.milestone_type).cmp(&stage_order(&b.milestone_type)))
    });
    let projects_affected = items
        .iter()
        .filter(|item| item.changed)
        .map(|item| item.project_id)
        .collect::<HashSet<_>>()
        .len();
    let error_count = parsed
        .issues
        .iter()
        .filter(|issue| issue.severity == ImportIssueSeverity::Error)
        .count();
    let warning_count = parsed
        .issues
        .iter()
        .filter(|issue| issue.severity == ImportIssueSeverity::Warning)
        .count();
    Ok(PreparedMcr {
        preview: McrPreview {
            preview_token: String::new(),
            file_name: parsed.file_name,
            rows_detected: parsed.rows_detected,
            master_ids_detected: parsed.master_ids.len(),
            matched_projects: matched.len(),
            unmatched_master_ids: parsed.master_ids.len() - matched.len(),
            milestone_dates_detected: parsed.dates.len(),
            projects_affected,
            warning_count,
            error_count,
            items,
            issues: parsed.issues,
        },
        file_size: parsed.file_size,
        base_revision,
        sync_revision,
    })
}

pub fn apply(
    connection: &mut Connection,
    prepared: &PreparedMcr,
) -> Result<McrApplyResult, ImportFailure> {
    if prepared.preview.error_count > 0 {
        return Err(ImportFailure::new(
            "validation_blocked",
            "MCR update is blocked by preview errors.",
            vec![],
        ));
    }
    let tx = connection.transaction().map_err(db_error)?;
    if revision(&tx, "import_history").map_err(db_error)? != prepared.base_revision
        || revision(&tx, "mcr_sync_history").map_err(db_error)? != prepared.sync_revision
    {
        return Err(stale_preview());
    }
    for item in &prepared.preview.items {
        let current: Option<(Option<String>, String)> = tx.query_row(
            "SELECT planned_date, plan_source FROM project_milestones WHERE project_id = ?1 AND milestone_type = ?2",
            params![item.project_id, item.milestone_type], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional().map_err(db_error)?;
        if current.as_ref()
            != Some(&(
                item.current_plan_date.clone(),
                item.current_plan_source.clone(),
            ))
        {
            return Err(stale_preview());
        }
    }
    let mut updated = 0;
    for item in prepared.preview.items.iter().filter(|item| item.changed) {
        let affected = tx.execute(
            "UPDATE project_milestones SET planned_date = ?1, plan_source = 'MCR', updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE project_id = ?2 AND milestone_type = ?3",
            params![item.new_plan_date, item.project_id, item.milestone_type],
        ).map_err(db_error)?;
        if affected != 1 {
            return Err(stale_preview());
        }
        updated += 1;
    }
    if updated > 0 {
        crate::risk_engine::recalculate_all(&tx).map_err(db_error)?;
        crate::insight_engine::recalculate_all(&tx).map_err(db_error)?;
        crate::risk_engine::mark_cache_current(&tx, Local::now().date_naive()).map_err(db_error)?;
    }
    let applied_at = Utc::now().to_rfc3339();
    tx.execute(
        "INSERT INTO mcr_sync_history (file_name, file_size, synced_at, master_ids, matched_projects, updated_milestones, projects_affected, warning_count) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![prepared.preview.file_name, prepared.file_size as i64, applied_at,
            prepared.preview.master_ids_detected as i64, prepared.preview.matched_projects as i64,
            updated as i64, prepared.preview.projects_affected as i64, prepared.preview.warning_count as i64],
    ).map_err(db_error)?;
    tx.commit().map_err(db_error)?;
    Ok(McrApplyResult {
        file_name: prepared.preview.file_name.clone(),
        updated_milestones: updated,
        projects_affected: prepared.preview.projects_affected,
        applied_at,
    })
}

fn revision(connection: &Connection, table: &str) -> rusqlite::Result<i64> {
    // Table names are fixed constants supplied by this module, never user input.
    connection.query_row(
        &format!("SELECT COALESCE(MAX(id), 0) FROM {table}"),
        [],
        |row| row.get(0),
    )
}

fn db_error(error: rusqlite::Error) -> ImportFailure {
    ImportFailure::new(
        "mcr_database_error",
        "MCR sync could not complete. No milestone changes were committed.",
        vec![error.to_string()],
    )
}

fn stale_preview() -> ImportFailure {
    ImportFailure::new(
        "stale_preview",
        "The project or milestone data changed after this preview. Preview the MCR file again.",
        vec![],
    )
}

fn parse(path: &Path) -> Result<ParsedMcr, ImportFailure> {
    validate_file(path)?;
    let mut workbook: Xlsx<_> = open_workbook(path).map_err(|error: XlsxError| {
        ImportFailure::new(
            "invalid_excel",
            "The MCR workbook could not be opened.",
            vec![error.to_string()],
        )
    })?;
    let sheet_names = workbook.sheet_names();
    if sheet_names.is_empty() {
        return Err(ImportFailure::new(
            "missing_sheet",
            "The MCR workbook does not contain a worksheet.",
            vec![],
        ));
    }
    let mut selected = None;
    let mut matching_sheets = Vec::new();
    for sheet in sheet_names {
        let mut reader = workbook.worksheet_cells_reader(&sheet).map_err(|error| {
            ImportFailure::new(
                "invalid_excel",
                format!("MCR worksheet '{sheet}' could not be read."),
                vec![error.to_string()],
            )
        })?;
        let mut cells = HashMap::<(u32, u32), SourceCell>::new();
        let mut max_row = 0;
        while let Some(cell) = reader.next_cell_with_formula().map_err(|error| {
            ImportFailure::new(
                "invalid_excel",
                format!("MCR worksheet '{sheet}' could not be read."),
                vec![error.to_string()],
            )
        })? {
            max_row = max_row.max(cell.pos.0);
            cells.insert(
                cell.pos,
                SourceCell {
                    value: cell.value.into(),
                    formula: cell.formula,
                },
            );
        }
        if let Some((headers, header_row)) =
            find_header_row(&cells, "MCR ID Level 0", &REQUIRED_HEADERS, true)
        {
            matching_sheets.push(sheet);
            if selected.is_none() {
                selected = Some((cells, max_row, headers, header_row));
            }
        }
    }
    if matching_sheets.len() > 1 {
        return Err(ImportFailure::new(
            "ambiguous_sheet",
            "More than one worksheet contains a complete MCR header. Keep only one MCR data sheet.",
            matching_sheets,
        ));
    }
    let (cells, max_row, headers, header_row) = selected.ok_or_else(|| {
        ImportFailure::new(
            "missing_mcr_columns",
            "No worksheet contains the required MCR headers.",
            REQUIRED_HEADERS.iter().map(|s| s.to_string()).collect(),
        )
    })?;
    let column = |name: &str| headers[&name.to_ascii_lowercase()];
    verify_mcr_slash_dates(&cells, column("Date formate"), header_row + 1, max_row)?;
    let mut rows = Vec::new();
    let mut master_ids = HashSet::new();
    let mut issues = Vec::new();
    let mut rows_detected = 0;
    for row in (header_row + 1)..=max_row {
        let source_row = row as i64 + 1;
        let values = REQUIRED_HEADERS.map(|name| cells.get(&(row, column(name))));
        if values
            .iter()
            .all(|cell| cell.is_none_or(|c| c.value.is_empty() && c.formula.is_none()))
        {
            continue;
        }
        rows_detected += 1;
        let level0 = required_text(values[1], source_row, "MCR ID Level 0", &mut issues);
        let Some(level0) = level0 else { continue };
        master_ids.insert(level0.clone());
        // Child ID is traceability only. The authoritative join key is Level 0.
        optional_text(values[0], source_row, "PRJ MCR ID K", &mut issues);
        let stage_text = optional_text(values[2], source_row, "WP: WPI-Nr ID T", &mut issues);
        let Some(stage_text) = stage_text else {
            continue;
        };
        if is_excluded_stage(&stage_text) {
            continue;
        }
        let Some(stage) = map_stage(&stage_text) else {
            issues.push(issue(
                ImportIssueSeverity::Warning,
                "unmapped_stage",
                Some(source_row),
                "WP: WPI-Nr ID T",
                Some(stage_text),
                None,
                "Stage is not one of SE2, SE3, SE4, QGC0 or QGC4; row was ignored.",
            ));
            continue;
        };
        let date = values[3]
            .and_then(|cell| parse_mcr_date_cell(cell, "Date formate", source_row, &mut issues));
        if let Some(date) = date {
            rows.push(McrRow {
                row: source_row,
                level0,
                stage,
                date,
            });
        }
    }
    if rows_detected == 0 {
        return Err(ImportFailure::new(
            "empty_mcr",
            "No MCR data rows were found.",
            vec![],
        ));
    }
    let dates = aggregate(rows);
    Ok(ParsedMcr {
        file_name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("MCR.xlsx")
            .to_string(),
        file_size: fs::metadata(path).map(|meta| meta.len()).unwrap_or(0),
        rows_detected,
        master_ids,
        dates,
        issues,
    })
}

fn verify_mcr_slash_dates(
    cells: &HashMap<(u32, u32), SourceCell>,
    date_column: u32,
    first_row: u32,
    last_row: u32,
) -> Result<(), ImportFailure> {
    let mut us_evidence = false;
    let mut ambiguous = false;
    for row in first_row..=last_row {
        let Some(value) = cells
            .get(&(row, date_column))
            .and_then(|cell| text(&cell.value))
        else {
            continue;
        };
        let parts: Vec<&str> = value.split('/').collect();
        if parts.len() != 3 || parts[0].len() == 4 {
            continue;
        }
        let (Ok(first), Ok(second)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) else {
            continue;
        };
        if first > 12 && second <= 12 {
            return Err(ImportFailure::new("unsupported_mcr_date_order",
                "MCR Date formate appears to contain DD/MM/YYYY values; this source requires confirmed MM/DD/YYYY dates.",
                vec![format!("Row {}: {value}", row + 1)]));
        }
        us_evidence |= first <= 12 && second > 12;
        ambiguous |= first <= 12 && second <= 12;
    }
    if ambiguous && !us_evidence {
        return Err(ImportFailure::new("ambiguous_mcr_date_order",
            "MCR slash dates are ambiguous; no MM/DD/YYYY example with a day above 12 was found. No dates were guessed.", vec![]));
    }
    Ok(())
}

fn aggregate(rows: Vec<McrRow>) -> Vec<Aggregated> {
    let mut groups = BTreeMap::<(String, &'static str), Aggregated>::new();
    for row in rows {
        let entry = groups
            .entry((row.level0.clone(), row.stage))
            .or_insert_with(|| Aggregated {
                level0: row.level0,
                stage: row.stage,
                latest_date: row.date.clone(),
                contributing_rows: Vec::new(),
            });
        if row.date > entry.latest_date {
            entry.latest_date = row.date;
        }
        entry.contributing_rows.push(row.row);
    }
    groups.into_values().collect()
}

fn map_stage(value: &str) -> Option<&'static str> {
    let normalized = value.trim().to_ascii_uppercase();
    STAGES.iter().copied().find(|stage| *stage == normalized)
}

fn is_excluded_stage(value: &str) -> bool {
    EXCLUDED_STAGES.contains(&value.trim().to_ascii_uppercase().as_str())
}

fn stage_order(value: &str) -> usize {
    STAGES
        .iter()
        .position(|stage| *stage == value)
        .unwrap_or(usize::MAX)
}

fn text(value: &Data) -> Option<String> {
    let value = match value {
        Data::String(v) | Data::DateTimeIso(v) => v.clone(),
        Data::Int(v) => v.to_string(),
        Data::Float(v) if v.fract() == 0.0 => format!("{v:.0}"),
        Data::Float(v) => v.to_string(),
        _ => return None,
    };
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn required_text(
    cell: Option<&SourceCell>,
    row: i64,
    field: &str,
    issues: &mut Vec<ImportIssue>,
) -> Option<String> {
    let value = cell.and_then(|cell| text(&cell.value));
    if let Some(value) = value {
        return Some(value);
    }
    let (category, original) = match cell {
        Some(SourceCell {
            value: Data::Error(error),
            formula,
            ..
        }) => (
            if formula.is_some() {
                "formula_error"
            } else {
                "cell_error"
            },
            Some(error.to_string()),
        ),
        Some(SourceCell {
            formula: Some(_), ..
        }) => ("formula_missing_cache", None),
        _ => ("required_value", None),
    };
    issues.push(issue(
        ImportIssueSeverity::Error,
        category,
        Some(row),
        field,
        original,
        None,
        "Required MCR value is missing or has no usable cached formula result.",
    ));
    None
}

fn optional_text(
    cell: Option<&SourceCell>,
    row: i64,
    field: &str,
    issues: &mut Vec<ImportIssue>,
) -> Option<String> {
    if let Some(value) = cell.and_then(|cell| text(&cell.value)) {
        return Some(value);
    }
    let (category, original) = match cell {
        Some(SourceCell {
            value: Data::Error(error),
            formula,
            ..
        }) => (
            if formula.is_some() {
                "formula_error"
            } else {
                "cell_error"
            },
            Some(error.to_string()),
        ),
        Some(SourceCell {
            formula: Some(_), ..
        }) => ("formula_missing_cache", None),
        _ => ("missing_value", None),
    };
    issues.push(issue(
        ImportIssueSeverity::Warning,
        category,
        Some(row),
        field,
        original,
        None,
        "MCR value is missing or invalid; Level 0 remains the only project matching key.",
    ));
    None
}

fn issue(
    severity: ImportIssueSeverity,
    category: &str,
    row: Option<i64>,
    field: &str,
    original_value: Option<String>,
    normalized_value: Option<String>,
    message: &str,
) -> ImportIssue {
    ImportIssue {
        severity,
        category: category.to_string(),
        row,
        field: Some(field.to_string()),
        original_value,
        normalized_value,
        issue: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        database::migrations,
        importer::{parse_workbook, replace_dataset},
    };
    use calamine::CellErrorType;
    #[test]
    fn latest_date_is_selected_independent_of_row_order() {
        let rows = vec![
            McrRow {
                row: 2,
                level0: "BM-1".into(),
                stage: "SE3",
                date: "2026-04-15".into(),
            },
            McrRow {
                row: 3,
                level0: "BM-1".into(),
                stage: "SE3",
                date: "2026-03-01".into(),
            },
            McrRow {
                row: 4,
                level0: "BM-1".into(),
                stage: "SE3",
                date: "2026-04-03".into(),
            },
        ];
        let result = aggregate(rows);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].latest_date, "2026-04-15");
        assert_eq!(result[0].contributing_rows, vec![2, 3, 4]);
    }
    #[test]
    fn only_confirmed_stages_are_mapped() {
        assert_eq!(map_stage(" se2 "), Some("SE2"));
        assert_eq!(map_stage("QG0"), None);
        assert!(is_excluded_stage(" qg0 "));
        assert_eq!(map_stage("SE4_QGC0"), None);
    }
    #[test]
    fn real_mcr_sample_excludes_qg0_without_warning() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("MCR_Fake.xlsx");
        let parsed = parse(&path).expect("MCR sample should parse");
        assert_eq!(parsed.rows_detected, 300);
        assert_eq!(parsed.master_ids.len(), 50);
        assert_eq!(parsed.dates.len(), 250);
        assert_eq!(
            parsed
                .issues
                .iter()
                .filter(|issue| issue.category == "unmapped_stage")
                .count(),
            0
        );
    }

    #[test]
    fn accepts_macro_enabled_mcr_extension() {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("MCR_Fake.xlsx");
        let temporary = std::env::temp_dir().join(format!(
            "project-risk-mcr-{}-{}.xlsm",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::copy(source, &temporary).expect("test workbook should be copied");
        let result = parse(&temporary);
        std::fs::remove_file(&temporary).expect("temporary workbook should be removed");
        assert!(
            result.is_ok(),
            ".xlsm MCR workbook should parse: {result:?}"
        );
    }

    #[test]
    fn finds_mcr_data_after_cover_worksheet() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("mcr-second-sheet.xlsx");
        let result = parse(&path).expect("later MCR worksheet should parse");
        assert_eq!(result.rows_detected, 1);
        assert_eq!(result.dates.len(), 1);
        assert_eq!(result.dates[0].stage, "SE2");
        assert_eq!(result.dates[0].latest_date, "2026-04-15");
    }

    #[test]
    fn formula_cache_and_error_are_handled_without_formula_evaluation() {
        let mut issues = Vec::new();
        let cached = SourceCell {
            value: Data::String("05/13/2025".into()),
            formula: Some("=A1".into()),
        };
        assert_eq!(
            parse_mcr_date_cell(&cached, "Date formate", 2, &mut issues).as_deref(),
            Some("2025-05-13")
        );
        let blank = SourceCell {
            value: Data::Empty,
            formula: Some("=A2".into()),
        };
        assert_eq!(
            parse_mcr_date_cell(&blank, "Date formate", 3, &mut issues),
            None
        );
        let error = SourceCell {
            value: Data::Error(CellErrorType::NA),
            formula: Some("=A3".into()),
        };
        assert_eq!(
            parse_mcr_date_cell(&error, "Date formate", 4, &mut issues),
            None
        );
        assert!(issues
            .iter()
            .any(|issue| issue.category == "formula_missing_cache"));
        assert!(issues.iter().any(|issue| issue.category == "formula_error"));
    }

    #[test]
    fn ambiguous_or_european_slash_order_is_rejected_before_import() {
        let ambiguous = HashMap::from([(
            (1, 13),
            SourceCell {
                value: Data::String("05/11/2026".into()),
                formula: None,
            },
        )]);
        assert_eq!(
            verify_mcr_slash_dates(&ambiguous, 13, 1, 1)
                .unwrap_err()
                .code,
            "ambiguous_mcr_date_order"
        );
        let european = HashMap::from([(
            (1, 13),
            SourceCell {
                value: Data::String("13/05/2026".into()),
                formula: None,
            },
        )]);
        assert_eq!(
            verify_mcr_slash_dates(&european, 13, 1, 1)
                .unwrap_err()
                .code,
            "unsupported_mcr_date_order"
        );
    }

    fn seeded_connection() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .unwrap();
        migrations::run(&mut connection).unwrap();
        let base = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("Active_Project_List_Fake.xlsx");
        let dataset = parse_workbook(&base).unwrap();
        replace_dataset(&mut connection, &dataset, 0, 0).unwrap();
        connection.execute(
            "UPDATE project_milestones SET planned_date = '2000-01-01', plan_source = 'Project List' WHERE project_id = (SELECT MIN(id) FROM projects) AND milestone_type = 'SE2'",
            [],
        ).unwrap();
        connection
    }

    fn sample_path() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("MCR_Fake.xlsx")
    }

    #[test]
    fn preview_and_apply_preserve_actual_dates_and_recalculate_risk() {
        let mut connection = seeded_connection();
        let prepared = prepare(&sample_path(), &connection).unwrap();
        assert_eq!(prepared.preview.master_ids_detected, 50);
        assert_eq!(prepared.preview.matched_projects, 42);
        assert_eq!(prepared.preview.unmatched_master_ids, 8);
        assert_eq!(prepared.preview.milestone_dates_detected, 250);
        assert_eq!(prepared.preview.items.len(), 210);
        assert_eq!(prepared.preview.error_count, 0);
        let original_actuals: Vec<(i64, String, Option<String>, String)> = connection.prepare(
            "SELECT project_id, milestone_type, actual_date, actual_source FROM project_milestones ORDER BY project_id, milestone_type"
        ).unwrap().query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).unwrap()
            .collect::<rusqlite::Result<_>>().unwrap();

        let result = apply(&mut connection, &prepared).unwrap();
        assert!(result.updated_milestones > 0);
        assert_eq!(result.projects_affected, prepared.preview.projects_affected);
        let after_actuals: Vec<(i64, String, Option<String>, String)> = connection.prepare(
            "SELECT project_id, milestone_type, actual_date, actual_source FROM project_milestones ORDER BY project_id, milestone_type"
        ).unwrap().query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).unwrap()
            .collect::<rusqlite::Result<_>>().unwrap();
        assert_eq!(original_actuals, after_actuals);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM mcr_sync_history", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM project_risks", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            42
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM project_milestones WHERE plan_source = 'MCR'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            210
        );
        assert!(
            crate::risk_engine::cache_is_current(&connection, Local::now().date_naive()).unwrap()
        );
        let second = prepare(&sample_path(), &connection).unwrap();
        assert!(second.preview.items.iter().all(|item| !item.changed));
        assert_eq!(
            apply(&mut connection, &prepared).unwrap_err().code,
            "stale_preview"
        );
        let no_op = apply(&mut connection, &second).unwrap();
        assert_eq!(no_op.updated_milestones, 0);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM mcr_sync_history", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            2
        );
    }

    #[test]
    fn failed_update_rolls_back_all_milestone_changes() {
        let mut connection = seeded_connection();
        let prepared = prepare(&sample_path(), &connection).unwrap();
        let first = prepared
            .preview
            .items
            .iter()
            .find(|item| item.changed)
            .unwrap();
        let before: (Option<String>, String) = connection.query_row(
            "SELECT planned_date, plan_source FROM project_milestones WHERE project_id = ?1 AND milestone_type = ?2",
            params![first.project_id, first.milestone_type], |row| Ok((row.get(0)?, row.get(1)?))
        ).unwrap();
        connection.execute_batch("CREATE TEMP TRIGGER reject_mcr BEFORE UPDATE OF planned_date ON project_milestones BEGIN SELECT RAISE(ABORT, 'injected failure'); END;").unwrap();
        assert_eq!(
            apply(&mut connection, &prepared).unwrap_err().code,
            "mcr_database_error"
        );
        let after: (Option<String>, String) = connection.query_row(
            "SELECT planned_date, plan_source FROM project_milestones WHERE project_id = ?1 AND milestone_type = ?2",
            params![first.project_id, first.milestone_type], |row| Ok((row.get(0)?, row.get(1)?))
        ).unwrap();
        assert_eq!(before, after);
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM mcr_sync_history", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
