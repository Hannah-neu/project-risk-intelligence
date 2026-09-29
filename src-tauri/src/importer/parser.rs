use super::{
    validate_file, ImportFailure, ImportIssue, ImportIssueSeverity, ImportPreview, ParsedDataset,
    ParsedMilestone, ParsedProject, PreparedImport,
};
use calamine::{open_workbook, Data, DataType, Reader, Xlsx, XlsxError};
use chrono::NaiveDate;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

const EXPECTED_COLUMNS: [&str; 35] = [
    "No",
    "PRJ MCR-ID",
    "PRJ name",
    "Customer",
    "P-ID",
    "Acq manager",
    "Project manager",
    "Engine Segment (groups)",
    "Prod Category",
    "Emission",
    "(A)PIC",
    "A/K Project",
    "System",
    "Components",
    "New released components",
    "injector",
    "Pump",
    "Rail",
    "ECU",
    "DNOX",
    "Other Component",
    "PRJ Status",
    "SE2 plan",
    "SE3 plan",
    "SE4 plan",
    "QGC0 plan",
    "QGC4 plan",
    "SE2 Act",
    "SE3 Act",
    "SE4 Act",
    "QGC0 Act",
    "QGC4 Act",
    "B sample",
    "C sample",
    "D sample",
];
pub(crate) const MILESTONES: [(&str, &str, &str); 5] = [
    ("SE2", "SE2 plan", "SE2 Act"),
    ("SE3", "SE3 plan", "SE3 Act"),
    ("SE4", "SE4 plan", "SE4 Act"),
    ("QGC0", "QGC0 plan", "QGC0 Act"),
    ("QGC4", "QGC4 plan", "QGC4 Act"),
];
pub(crate) const COMPONENT_SEPARATORS: [char; 4] = [',', ';', '，', '；'];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SlashDateOrder {
    #[default]
    Unknown,
    MonthDay,
    DayMonth,
}

#[derive(Clone, Debug, Default)]
pub(super) struct SourceCell {
    pub(super) value: Data,
    pub(super) formula: Option<String>,
}

pub fn prepare_workbook(path: &Path) -> Result<PreparedImport, ImportFailure> {
    parse_and_validate(path)
}

#[cfg(test)]
pub fn parse_workbook(path: &Path) -> Result<ParsedDataset, ImportFailure> {
    let outcome = parse_and_validate(path)?;
    if !outcome.preview.validation_passed() {
        return Err(ImportFailure::new(
            "validation_failed",
            "The workbook contains errors that must be resolved before import.",
            outcome
                .preview
                .issues
                .iter()
                .filter(|issue| issue.severity == ImportIssueSeverity::Error)
                .take(20)
                .map(format_issue)
                .collect(),
        ));
    }
    Ok(outcome.dataset)
}

fn parse_and_validate(path: &Path) -> Result<PreparedImport, ImportFailure> {
    validate_file(path)?;
    let mut workbook: Xlsx<_> = open_workbook(path).map_err(|error: XlsxError| {
        ImportFailure::new(
            "invalid_excel",
            "The workbook could not be opened.",
            vec![error.to_string()],
        )
    })?;
    let sheet_names = workbook.sheet_names();
    if sheet_names.is_empty() {
        return Err(ImportFailure::new(
            "missing_sheet",
            "The workbook does not contain a worksheet.",
            Vec::new(),
        ));
    }
    let mut selected = None;
    let mut matching_sheets = Vec::new();
    for sheet_name in sheet_names {
        let mut reader = workbook
            .worksheet_cells_reader(&sheet_name)
            .map_err(|error| {
                ImportFailure::new(
                    "invalid_excel",
                    format!("Worksheet '{sheet_name}' could not be read."),
                    vec![error.to_string()],
                )
            })?;
        let mut cells = HashMap::<(u32, u32), SourceCell>::new();
        let mut max_row = 0_u32;
        while let Some(cell) = reader.next_cell_with_formula().map_err(|error| {
            ImportFailure::new(
                "invalid_excel",
                format!("Worksheet '{sheet_name}' could not be read."),
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
            find_header_row(&cells, "PRJ MCR-ID", &EXPECTED_COLUMNS, false)
        {
            matching_sheets.push(sheet_name);
            if selected.is_none() {
                selected = Some((cells, max_row, headers, header_row));
            }
        }
    }
    if matching_sheets.len() > 1 {
        return Err(ImportFailure::new(
            "ambiguous_sheet",
            "More than one worksheet contains a complete Project List header. Keep only one Project List sheet.",
            matching_sheets,
        ));
    }
    let (cells, max_row, headers, header_row) = selected.ok_or_else(|| {
        ImportFailure::new(
            "missing_columns",
            "No worksheet contains the required Project List headers.",
            EXPECTED_COLUMNS
                .iter()
                .map(|column| (*column).to_string())
                .collect(),
        )
    })?;
    if max_row <= header_row {
        return Err(ImportFailure::new(
            "empty_dataset",
            "The workbook does not contain project records.",
            Vec::new(),
        ));
    }

    let source_file = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("import.xlsx")
        .to_string();
    let file_size = fs::metadata(path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let mut issues = Vec::new();
    let mut projects = Vec::new();
    let mut detected_rows = Vec::new();
    // Project List convention: non-year-first slash dates are MM/DD/YYYY.
    let slash_date_order = SlashDateOrder::MonthDay;

    for row_index in (header_row + 1)..=max_row {
        if row_is_empty(&cells, row_index, &headers) {
            continue;
        }
        let source_row = row_index as i64 + 1;
        detected_rows.push(source_row);
        projects.push(parse_project(
            &cells,
            &headers,
            row_index,
            source_row,
            slash_date_order,
            &mut issues,
        ));
    }

    if detected_rows.is_empty() {
        return Err(ImportFailure::new(
            "empty_dataset",
            "No project records were found after the header rows.",
            Vec::new(),
        ));
    }

    add_duplicate_issues(&projects, &mut issues);
    let invalid_rows: HashSet<i64> = issues
        .iter()
        .filter(|issue| issue.severity == ImportIssueSeverity::Error)
        .filter_map(|issue| issue.row)
        .collect();
    let valid_records = detected_rows
        .iter()
        .filter(|row| !invalid_rows.contains(row))
        .count();
    let error_count = count_severity(&issues, ImportIssueSeverity::Error);
    let warning_count = count_severity(&issues, ImportIssueSeverity::Warning);
    let info_count = count_severity(&issues, ImportIssueSeverity::Info);
    let duplicate_mcr_ids = issues
        .iter()
        .filter(|issue| issue.category == "duplicate_mcr_id")
        .filter_map(|issue| issue.original_value.as_deref())
        .collect::<HashSet<_>>()
        .len();
    let formula_errors = issues
        .iter()
        .filter(|issue| issue.category.starts_with("formula_"))
        .count();
    let normalized_dates = issues
        .iter()
        .filter(|issue| issue.category == "normalized_date")
        .count();
    let normalized_component_lists = issues
        .iter()
        .filter(|issue| issue.category == "normalized_components")
        .count();

    Ok(PreparedImport {
        dataset: ParsedDataset {
            source_file: source_file.clone(),
            file_size,
            projects,
        },
        preview: ImportPreview {
            preview_token: String::new(),
            file_name: source_file,
            projects_detected: detected_rows.len(),
            valid_records,
            error_count,
            warning_count,
            info_count,
            duplicate_mcr_ids,
            formula_errors,
            normalized_dates,
            normalized_component_lists,
            issues,
        },
    })
}

fn parse_project(
    cells: &HashMap<(u32, u32), SourceCell>,
    columns: &HashMap<String, u32>,
    row_index: u32,
    row_number: i64,
    slash_date_order: SlashDateOrder,
    issues: &mut Vec<ImportIssue>,
) -> ParsedProject {
    let mcr_id = read_text(
        cells,
        columns,
        row_index,
        "PRJ MCR-ID",
        true,
        row_number,
        issues,
    )
    .unwrap_or_default();
    let project_name = read_text(
        cells, columns, row_index, "PRJ name", true, row_number, issues,
    )
    .unwrap_or_default();

    let components_raw = read_text(
        cells,
        columns,
        row_index,
        "Components",
        false,
        row_number,
        issues,
    );
    let (components, components_list) =
        normalize_components(components_raw, "Components", row_number, issues);
    let released_raw = read_text(
        cells,
        columns,
        row_index,
        "New released components",
        false,
        row_number,
        issues,
    );
    let (new_released_components, new_released_components_list) =
        normalize_components(released_raw, "New released components", row_number, issues);

    let project_status = normalize_project_status(read_text(
        cells,
        columns,
        row_index,
        "PRJ Status",
        false,
        row_number,
        issues,
    ));
    if let Some(status) = project_status.as_deref() {
        let known = [
            "ACTIVE",
            "ON HOLD",
            "HOLD",
            "CLOSED",
            "COMPLETED",
            "CANCELLED",
            "CANCELED",
            "END",
            "LOST",
            "PLAN",
            "PREP",
            "RESV",
            "WISH",
            "WORK",
        ];
        if !known.contains(&status) {
            push_issue(
                issues,
                ImportIssueSeverity::Warning,
                "unknown_status",
                row_number,
                "PRJ Status",
                Some(status.to_string()),
                Some(status.to_string()),
                "Unknown project status was preserved for review.",
            );
        }
    }

    let milestones = MILESTONES
        .iter()
        .map(
            |(milestone_type, plan_column, actual_column)| ParsedMilestone {
                milestone_type,
                planned_date: parse_date_at_with_order(
                    cells,
                    columns,
                    row_index,
                    plan_column,
                    row_number,
                    slash_date_order,
                    issues,
                ),
                actual_date: parse_date_at_with_order(
                    cells,
                    columns,
                    row_index,
                    actual_column,
                    row_number,
                    slash_date_order,
                    issues,
                ),
                plan_source: source_label(cells, columns, plan_column),
                actual_source: source_label(cells, columns, actual_column),
            },
        )
        .collect();

    ParsedProject {
        project_no: cell_at(cells, columns, row_index, "No").and_then(|cell| cell.value.as_i64()),
        mcr_id,
        project_name,
        customer: read_text(
            cells, columns, row_index, "Customer", false, row_number, issues,
        ),
        p_id: read_text(cells, columns, row_index, "P-ID", false, row_number, issues),
        acq_manager: read_text(
            cells,
            columns,
            row_index,
            "Acq manager",
            false,
            row_number,
            issues,
        ),
        project_manager: read_text(
            cells,
            columns,
            row_index,
            "Project manager",
            false,
            row_number,
            issues,
        ),
        engine_segment: read_text(
            cells,
            columns,
            row_index,
            "Engine Segment (groups)",
            false,
            row_number,
            issues,
        ),
        product_category: read_text(
            cells,
            columns,
            row_index,
            "Prod Category",
            false,
            row_number,
            issues,
        ),
        emission: read_text(
            cells, columns, row_index, "Emission", false, row_number, issues,
        ),
        apic: read_text(
            cells, columns, row_index, "(A)PIC", false, row_number, issues,
        ),
        project_type: read_text(
            cells,
            columns,
            row_index,
            "A/K Project",
            false,
            row_number,
            issues,
        ),
        system: read_text(
            cells, columns, row_index, "System", false, row_number, issues,
        ),
        components,
        new_released_components,
        components_list,
        new_released_components_list,
        has_injector: bool_at(cells, columns, row_index, "injector", row_number, issues),
        has_pump: bool_at(cells, columns, row_index, "Pump", row_number, issues),
        has_rail: bool_at(cells, columns, row_index, "Rail", row_number, issues),
        has_ecu: bool_at(cells, columns, row_index, "ECU", row_number, issues),
        has_dnox: bool_at(cells, columns, row_index, "DNOX", row_number, issues),
        other_component: read_text(
            cells,
            columns,
            row_index,
            "Other Component",
            false,
            row_number,
            issues,
        ),
        project_status,
        b_sample: parse_date_at_with_order(
            cells,
            columns,
            row_index,
            "B sample",
            row_number,
            slash_date_order,
            issues,
        ),
        c_sample: parse_date_at_with_order(
            cells,
            columns,
            row_index,
            "C sample",
            row_number,
            slash_date_order,
            issues,
        ),
        d_sample: parse_date_at_with_order(
            cells,
            columns,
            row_index,
            "D sample",
            row_number,
            slash_date_order,
            issues,
        ),
        source_row: row_number,
        milestones,
    }
}

fn normalize_project_status(value: Option<String>) -> Option<String> {
    value
        .map(|status| status.trim().to_uppercase())
        .filter(|status| !status.is_empty())
}

pub(super) fn find_header_row(
    cells: &HashMap<(u32, u32), SourceCell>,
    anchor: &str,
    required: &[&str],
    case_insensitive: bool,
) -> Option<(HashMap<String, u32>, u32)> {
    let normalize = |value: &str| {
        if case_insensitive {
            value.to_ascii_lowercase()
        } else {
            value.to_string()
        }
    };
    let anchor = normalize(anchor);
    let mut candidates: Vec<u32> = cells
        .iter()
        .filter_map(|((row, _), cell)| {
            plain_text(&cell.value)
                .is_some_and(|value| normalize(&value) == anchor)
                .then_some(*row)
        })
        .collect();
    candidates.sort_unstable();
    candidates.dedup();
    candidates.into_iter().find_map(|row| {
        let headers: HashMap<String, u32> = cells
            .iter()
            .filter(|((cell_row, _), _)| *cell_row == row)
            .filter_map(|((_, column), cell)| {
                plain_text(&cell.value).map(|value| (normalize(&value), *column))
            })
            .collect();
        required
            .iter()
            .all(|name| headers.contains_key(&normalize(name)))
            .then_some((headers, row))
    })
}

fn cell_at<'a>(
    cells: &'a HashMap<(u32, u32), SourceCell>,
    columns: &HashMap<String, u32>,
    row: u32,
    column: &str,
) -> Option<&'a SourceCell> {
    columns
        .get(column)
        .and_then(|index| cells.get(&(row, *index)))
}

fn row_is_empty(
    cells: &HashMap<(u32, u32), SourceCell>,
    row: u32,
    columns: &HashMap<String, u32>,
) -> bool {
    columns.values().all(|column| {
        cells
            .get(&(row, *column))
            .is_none_or(|cell| cell.value.is_empty() && cell.formula.is_none())
    })
}

fn read_text(
    cells: &HashMap<(u32, u32), SourceCell>,
    columns: &HashMap<String, u32>,
    row: u32,
    column: &str,
    required: bool,
    row_number: i64,
    issues: &mut Vec<ImportIssue>,
) -> Option<String> {
    let Some(cell) = cell_at(cells, columns, row, column) else {
        if required {
            push_issue(
                issues,
                ImportIssueSeverity::Error,
                "required_value",
                row_number,
                column,
                None,
                None,
                "Required value is missing.",
            );
        }
        return None;
    };
    let severity = if required {
        ImportIssueSeverity::Error
    } else {
        ImportIssueSeverity::Warning
    };
    if let Data::Error(error) = &cell.value {
        push_issue(
            issues,
            severity,
            if cell.formula.is_some() {
                "formula_error"
            } else {
                "cell_error"
            },
            row_number,
            column,
            Some(error.to_string()),
            None,
            "Excel error value was normalized to null.",
        );
        return None;
    }
    let raw_value = raw_text(&cell.value);
    if raw_value
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        if cell.formula.is_some() {
            push_issue(
                issues,
                severity,
                "formula_missing_cache",
                row_number,
                column,
                None,
                None,
                "Formula has no usable cached result; the formula was not evaluated.",
            );
        } else if required {
            push_issue(
                issues,
                ImportIssueSeverity::Error,
                "required_value",
                row_number,
                column,
                None,
                None,
                "Required value is missing.",
            );
        }
        return None;
    }
    let raw_value = raw_value.unwrap();
    let value = if column == "PRJ MCR-ID" {
        raw_value.trim().to_string()
    } else {
        raw_value.clone()
    };
    if column == "PRJ MCR-ID" {
        if raw_value != value {
            push_issue(
                issues,
                ImportIssueSeverity::Info,
                "trimmed_mcr_id",
                row_number,
                column,
                Some(raw_value),
                Some(value.clone()),
                "Leading or trailing whitespace was removed from PRJ MCR-ID.",
            );
        }
    }
    Some(value)
}

fn plain_text(cell: &Data) -> Option<String> {
    let value = raw_text(cell)?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn raw_text(cell: &Data) -> Option<String> {
    match cell {
        Data::Empty | Data::Error(_) => None,
        Data::String(value) | Data::DateTimeIso(value) | Data::DurationIso(value) => {
            Some(value.clone())
        }
        Data::Int(value) => Some(value.to_string()),
        Data::Float(value) if value.fract() == 0.0 => Some(format!("{value:.0}")),
        Data::Float(value) => Some(value.to_string()),
        Data::Bool(value) => Some(if *value { "Y" } else { "N" }.to_string()),
        Data::DateTime(value) => Some(value.to_string()),
    }
}

fn bool_at(
    cells: &HashMap<(u32, u32), SourceCell>,
    columns: &HashMap<String, u32>,
    row: u32,
    column: &str,
    row_number: i64,
    issues: &mut Vec<ImportIssue>,
) -> bool {
    read_text(cells, columns, row, column, false, row_number, issues).is_some_and(|value| {
        matches!(
            value.trim().to_ascii_uppercase().as_str(),
            "Y" | "YES" | "TRUE" | "1"
        )
    })
}

#[cfg(test)]
fn parse_date_at(
    cells: &HashMap<(u32, u32), SourceCell>,
    columns: &HashMap<String, u32>,
    row: u32,
    column: &str,
    row_number: i64,
    issues: &mut Vec<ImportIssue>,
) -> Option<String> {
    parse_date_at_with_order(
        cells,
        columns,
        row,
        column,
        row_number,
        SlashDateOrder::Unknown,
        issues,
    )
}

fn parse_date_at_with_order(
    cells: &HashMap<(u32, u32), SourceCell>,
    columns: &HashMap<String, u32>,
    row: u32,
    column: &str,
    row_number: i64,
    slash_date_order: SlashDateOrder,
    issues: &mut Vec<ImportIssue>,
) -> Option<String> {
    let cell = cell_at(cells, columns, row, column)?;
    parse_date_cell_with_format(cell, column, row_number, issues, slash_date_order, false)
}

pub(super) fn parse_mcr_date_cell(
    cell: &SourceCell,
    column: &str,
    row_number: i64,
    issues: &mut Vec<ImportIssue>,
) -> Option<String> {
    // The inspected MCR source uses MM/DD/YYYY text, including unambiguous values such as 05/13/2025.
    parse_date_cell_with_format(
        cell,
        column,
        row_number,
        issues,
        SlashDateOrder::MonthDay,
        true,
    )
}

fn parse_date_cell_with_format(
    cell: &SourceCell,
    column: &str,
    row_number: i64,
    issues: &mut Vec<ImportIssue>,
    slash_date_order: SlashDateOrder,
    mcr_us_slash: bool,
) -> Option<String> {
    if let Data::Error(error) = &cell.value {
        push_issue(
            issues,
            ImportIssueSeverity::Warning,
            if cell.formula.is_some() {
                "formula_error"
            } else {
                "cell_error"
            },
            row_number,
            column,
            Some(error.to_string()),
            None,
            "Excel error in an optional date was normalized to null.",
        );
        return None;
    }
    if cell.value.is_empty() || plain_text(&cell.value).is_none() {
        if cell.formula.is_some() {
            push_issue(
                issues,
                ImportIssueSeverity::Warning,
                "formula_missing_cache",
                row_number,
                column,
                None,
                None,
                "Formula returned blank or has no cached result; optional date was set to null.",
            );
        }
        return None;
    }
    if let Some(date) = cell.value.as_date() {
        let normalized = date.format("%Y-%m-%d").to_string();
        let original = raw_text(&cell.value);
        if original.as_deref() != Some(normalized.as_str()) {
            push_issue(
                issues,
                ImportIssueSeverity::Info,
                "normalized_date",
                row_number,
                column,
                original,
                Some(normalized.clone()),
                "Date was normalized to ISO YYYY-MM-DD.",
            );
        }
        return Some(normalized);
    }

    let value = plain_text(&cell.value)?;
    if is_null_like(&value) {
        push_issue(
            issues,
            ImportIssueSeverity::Warning,
            "normalized_date",
            row_number,
            column,
            Some(value),
            None,
            "Null-like date placeholder was normalized to null.",
        );
        return None;
    }
    if let Some((first, second)) = non_year_first_slash_parts(&value) {
        let resolved_order = if first > 12 && second <= 12 {
            Some(SlashDateOrder::DayMonth)
        } else if first <= 12 && second > 12 {
            Some(SlashDateOrder::MonthDay)
        } else if first <= 12 && second <= 12 {
            match slash_date_order {
                SlashDateOrder::MonthDay => Some(SlashDateOrder::MonthDay),
                SlashDateOrder::DayMonth => Some(SlashDateOrder::DayMonth),
                SlashDateOrder::Unknown => None,
            }
        } else {
            Some(SlashDateOrder::Unknown)
        };
        let Some(resolved_order) = resolved_order else {
            push_issue(
                issues,
                ImportIssueSeverity::Warning,
                "ambiguous_date",
                row_number,
                column,
                Some(value),
                None,
                "Ambiguous slash date was not guessed and was normalized to null.",
            );
            return None;
        };
        let format = if resolved_order == SlashDateOrder::MonthDay {
            "%m/%d/%Y"
        } else {
            "%d/%m/%Y"
        };
        if let Ok(date) = NaiveDate::parse_from_str(&value, format) {
            let normalized = date.format("%Y-%m-%d").to_string();
            push_issue(
                issues,
                ImportIssueSeverity::Info,
                "normalized_date",
                row_number,
                column,
                Some(value),
                Some(normalized.clone()),
                if resolved_order == SlashDateOrder::MonthDay {
                    "MM/DD/YYYY date was normalized to ISO YYYY-MM-DD."
                } else {
                    "DD/MM/YYYY date was normalized to ISO YYYY-MM-DD."
                },
            );
            return Some(normalized);
        }
        push_issue(
            issues,
            ImportIssueSeverity::Warning,
            "invalid_date",
            row_number,
            column,
            Some(value),
            None,
            if mcr_us_slash {
                "MCR date is not a valid MM/DD/YYYY value; no plan update was made."
            } else {
                "Invalid optional date was normalized to null."
            },
        );
        return None;
    }
    for format in ["%Y-%m-%d", "%Y/%m/%d", "%d/%m/%Y", "%d.%m.%Y"] {
        if let Ok(date) = NaiveDate::parse_from_str(&value, format) {
            let normalized = date.format("%Y-%m-%d").to_string();
            if value != normalized {
                push_issue(
                    issues,
                    ImportIssueSeverity::Info,
                    "normalized_date",
                    row_number,
                    column,
                    Some(value),
                    Some(normalized.clone()),
                    "Date was normalized to ISO YYYY-MM-DD.",
                );
            }
            return Some(normalized);
        }
    }
    push_issue(
        issues,
        ImportIssueSeverity::Warning,
        "invalid_date",
        row_number,
        column,
        Some(value),
        None,
        "Invalid optional date was normalized to null.",
    );
    None
}

fn is_null_like(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_uppercase().as_str(),
        "N/A" | "NA" | "N.A" | "N.A." | "#N/A" | "-" | "--" | "TBD" | "TBC"
    )
}

fn non_year_first_slash_parts(value: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = value.split('/').collect();
    if parts.len() != 3 || parts[0].len() == 4 {
        return None;
    }
    Some((parts[0].parse().ok()?, parts[1].parse().ok()?))
}

fn normalize_components(
    original: Option<String>,
    field: &str,
    row: i64,
    issues: &mut Vec<ImportIssue>,
) -> (Option<String>, Vec<String>) {
    let Some(original) = original else {
        return (None, Vec::new());
    };
    if original.contains('/') {
        push_issue(
            issues,
            ImportIssueSeverity::Warning,
            "unknown_component_separator",
            row,
            field,
            Some(original.clone()),
            Some(original.clone()),
            "Unknown '/' separator was preserved; review this component list.",
        );
    }
    let mut seen = HashSet::new();
    let values: Vec<String> = original
        .split(COMPONENT_SEPARATORS)
        .map(|value| value.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert(value.clone()))
        .collect();
    let normalized = values.join(", ");
    if normalized != original {
        push_issue(
            issues,
            ImportIssueSeverity::Info,
            "normalized_components",
            row,
            field,
            Some(original),
            Some(normalized.clone()),
            "Component separators, spacing, empty items, or duplicates were normalized.",
        );
    }
    ((!normalized.is_empty()).then_some(normalized), values)
}

fn add_duplicate_issues(projects: &[ParsedProject], issues: &mut Vec<ImportIssue>) {
    let mut rows_by_id: HashMap<&str, Vec<i64>> = HashMap::new();
    for project in projects.iter().filter(|project| !project.mcr_id.is_empty()) {
        rows_by_id
            .entry(project.mcr_id.as_str())
            .or_default()
            .push(project.source_row);
    }
    for (mcr_id, rows) in rows_by_id.into_iter().filter(|(_, rows)| rows.len() > 1) {
        for row in &rows {
            push_issue(
                issues,
                ImportIssueSeverity::Error,
                "duplicate_mcr_id",
                *row,
                "PRJ MCR-ID",
                Some(mcr_id.to_string()),
                Some(mcr_id.to_string()),
                &format!(
                    "Duplicate normalized PRJ MCR-ID appears on rows {}.",
                    rows.iter()
                        .map(i64::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            );
        }
    }
}

fn source_label(
    cells: &HashMap<(u32, u32), SourceCell>,
    columns: &HashMap<String, u32>,
    column: &str,
) -> String {
    cell_at(cells, columns, 0, column)
        .and_then(|cell| plain_text(&cell.value))
        .unwrap_or_else(|| "Unknown".to_string())
}

fn push_issue(
    issues: &mut Vec<ImportIssue>,
    severity: ImportIssueSeverity,
    category: &str,
    row: i64,
    field: &str,
    original_value: Option<String>,
    normalized_value: Option<String>,
    issue: &str,
) {
    issues.push(ImportIssue {
        severity,
        category: category.to_string(),
        row: Some(row),
        field: Some(field.to_string()),
        original_value,
        normalized_value,
        issue: issue.to_string(),
    });
}

fn count_severity(issues: &[ImportIssue], severity: ImportIssueSeverity) -> usize {
    issues
        .iter()
        .filter(|issue| issue.severity == severity)
        .count()
}

#[cfg(test)]
fn format_issue(issue: &ImportIssue) -> String {
    format!(
        "Row {}, {}: {}",
        issue
            .row
            .map(|row| row.to_string())
            .unwrap_or_else(|| "—".to_string()),
        issue.field.as_deref().unwrap_or("Workbook"),
        issue.issue
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::CellErrorType;

    fn formula_cell(value: Data) -> SourceCell {
        SourceCell {
            value,
            formula: Some("TEST()".to_string()),
        }
    }

    #[test]
    fn formula_cached_text_is_used_but_formula_blank_is_reported() {
        let columns = HashMap::from([("PRJ MCR-ID".to_string(), 0)]);
        let mut issues = Vec::new();
        let cells = HashMap::from([((2, 0), formula_cell(Data::String(" MCR-1 ".into())))]);
        assert_eq!(
            read_text(&cells, &columns, 2, "PRJ MCR-ID", true, 3, &mut issues).as_deref(),
            Some("MCR-1")
        );

        let cells = HashMap::from([((3, 0), formula_cell(Data::Empty))]);
        assert!(read_text(&cells, &columns, 3, "PRJ MCR-ID", true, 4, &mut issues).is_none());
        assert!(issues
            .iter()
            .any(|issue| issue.category == "formula_missing_cache"
                && issue.severity == ImportIssueSeverity::Error));
    }

    #[test]
    fn formula_error_and_blank_optional_dates_become_warnings() {
        let columns = HashMap::from([("SE2 plan".to_string(), 0)]);
        let mut issues = Vec::new();
        let error_cells = HashMap::from([((2, 0), formula_cell(Data::Error(CellErrorType::NA)))]);
        assert_eq!(
            parse_date_at(&error_cells, &columns, 2, "SE2 plan", 3, &mut issues),
            None
        );
        let blank_cells = HashMap::from([((3, 0), formula_cell(Data::String(String::new())))]);
        assert_eq!(
            parse_date_at(&blank_cells, &columns, 3, "SE2 plan", 4, &mut issues),
            None
        );
        assert_eq!(count_severity(&issues, ImportIssueSeverity::Warning), 2);
    }

    #[test]
    fn dates_support_serial_iso_and_reject_ambiguous_slashes() {
        let columns = HashMap::from([("SE2 plan".to_string(), 0)]);
        let mut issues = Vec::new();
        let serial = HashMap::from([(
            (2, 0),
            SourceCell {
                value: Data::Float(46023.0),
                formula: None,
            },
        )]);
        assert_eq!(
            parse_date_at(&serial, &columns, 2, "SE2 plan", 3, &mut issues).as_deref(),
            Some("2026-01-01")
        );
        let iso = HashMap::from([((3, 0), formula_cell(Data::String("2026-02-19".into())))]);
        assert_eq!(
            parse_date_at(&iso, &columns, 3, "SE2 plan", 4, &mut issues).as_deref(),
            Some("2026-02-19")
        );
        let ambiguous = HashMap::from([((4, 0), formula_cell(Data::String("01/02/2026".into())))]);
        assert_eq!(
            parse_date_at(&ambiguous, &columns, 4, "SE2 plan", 5, &mut issues),
            None
        );
        assert!(issues
            .iter()
            .any(|issue| issue.category == "ambiguous_date"));
    }

    #[test]
    fn project_list_uses_us_slash_dates_and_supports_year_first_slashes() {
        let columns = HashMap::from([
            ("SE2 plan".to_string(), 0),
            ("SE3 plan".to_string(), 1),
            ("SE4 plan".to_string(), 2),
        ]);
        let cells = HashMap::from([
            ((2, 0), formula_cell(Data::String("07/30/2026".into()))),
            ((3, 1), formula_cell(Data::String("02/01/2019".into()))),
            ((4, 2), formula_cell(Data::String("2026/08/31".into()))),
        ]);
        let order = SlashDateOrder::MonthDay;
        let mut issues = Vec::new();
        assert_eq!(
            parse_date_at_with_order(&cells, &columns, 2, "SE2 plan", 3, order, &mut issues)
                .as_deref(),
            Some("2026-07-30")
        );
        assert_eq!(
            parse_date_at_with_order(&cells, &columns, 3, "SE3 plan", 4, order, &mut issues)
                .as_deref(),
            Some("2019-02-01")
        );
        assert_eq!(
            parse_date_at_with_order(&cells, &columns, 4, "SE4 plan", 5, order, &mut issues)
                .as_deref(),
            Some("2026-08-31")
        );
        assert!(!issues
            .iter()
            .any(|issue| issue.severity == ImportIssueSeverity::Warning));
    }

    #[test]
    fn component_lists_normalize_supported_separators_and_preserve_slashes() {
        let mut issues = Vec::new();
        let (_, comma_list) =
            normalize_components(Some("Injector, Pump".into()), "Components", 2, &mut issues);
        assert_eq!(comma_list, vec!["Injector", "Pump"]);
        let (_, semicolon_list) =
            normalize_components(Some("Injector; Pump".into()), "Components", 2, &mut issues);
        assert_eq!(semicolon_list, vec!["Injector", "Pump"]);
        let (value, list) = normalize_components(
            Some("Injector； Pump,Injector，， Rail".into()),
            "Components",
            3,
            &mut issues,
        );
        assert_eq!(value.as_deref(), Some("Injector, Pump, Rail"));
        assert_eq!(list, vec!["Injector", "Pump", "Rail"]);
        let (value, _) =
            normalize_components(Some("Pump/Rail".into()), "Components", 4, &mut issues);
        assert_eq!(value.as_deref(), Some("Pump/Rail"));
        assert!(issues
            .iter()
            .any(|issue| issue.category == "unknown_component_separator"));
    }

    #[test]
    fn duplicate_mcr_ids_report_every_involved_row() {
        fn project(id: &str, row: i64) -> ParsedProject {
            ParsedProject {
                project_no: None,
                mcr_id: id.into(),
                project_name: "P".into(),
                customer: None,
                p_id: None,
                acq_manager: None,
                project_manager: None,
                engine_segment: None,
                product_category: None,
                emission: None,
                apic: None,
                project_type: None,
                system: None,
                components: None,
                new_released_components: None,
                components_list: vec![],
                new_released_components_list: vec![],
                has_injector: false,
                has_pump: false,
                has_rail: false,
                has_ecu: false,
                has_dnox: false,
                other_component: None,
                project_status: None,
                b_sample: None,
                c_sample: None,
                d_sample: None,
                source_row: row,
                milestones: vec![],
            }
        }
        let mut issues = Vec::new();
        add_duplicate_issues(&[project("MCR-1", 3), project("MCR-1", 9)], &mut issues);
        assert_eq!(issues.len(), 2);
        assert_eq!(
            issues
                .iter()
                .filter_map(|issue| issue.row)
                .collect::<HashSet<_>>(),
            HashSet::from([3, 9])
        );
    }

    #[test]
    fn null_like_date_placeholders_become_null() {
        for placeholder in ["N/A", "na", "N.A.", "-", "--", "TBD", "tbc"] {
            assert!(is_null_like(placeholder));
        }
    }

    #[test]
    fn project_status_normalization_merges_case_and_whitespace_variants() {
        for value in ["WORK", "work", " Work ", "wOrK"] {
            assert_eq!(
                normalize_project_status(Some(value.to_string())).as_deref(),
                Some("WORK")
            );
        }
        assert_eq!(normalize_project_status(Some("   ".to_string())), None);
    }

    #[test]
    fn blank_and_null_like_optional_milestones_do_not_block_import() {
        let columns = HashMap::from([("SE3 Act".to_string(), 0)]);
        let mut issues = Vec::new();
        let blank = HashMap::from([((2, 0), SourceCell::default())]);
        assert_eq!(
            parse_date_at(&blank, &columns, 2, "SE3 Act", 3, &mut issues),
            None
        );
        for (row, placeholder) in [(3, "n.a"), (4, "N/A")] {
            let cells = HashMap::from([(
                (row, 0),
                SourceCell {
                    value: Data::String(placeholder.into()),
                    formula: None,
                },
            )]);
            assert_eq!(
                parse_date_at(
                    &cells,
                    &columns,
                    row,
                    "SE3 Act",
                    row as i64 + 1,
                    &mut issues
                ),
                None
            );
        }
        assert_eq!(count_severity(&issues, ImportIssueSeverity::Error), 0);
        assert_eq!(count_severity(&issues, ImportIssueSeverity::Warning), 2);
    }

    #[test]
    fn headers_can_be_found_below_cover_rows_without_sheet_name_assumptions() {
        let cells = HashMap::from([
            (
                (0, 0),
                SourceCell {
                    value: Data::String("Cover page".into()),
                    formula: None,
                },
            ),
            (
                (8, 0),
                SourceCell {
                    value: Data::String("PRJ MCR-ID".into()),
                    formula: None,
                },
            ),
            (
                (8, 1),
                SourceCell {
                    value: Data::String("PRJ name".into()),
                    formula: None,
                },
            ),
        ]);
        let (headers, row) =
            find_header_row(&cells, "PRJ MCR-ID", &["PRJ MCR-ID", "PRJ name"], false)
                .expect("header row should be found");
        assert_eq!(row, 8);
        assert_eq!(headers["PRJ name"], 1);
        assert!(find_header_row(&cells, "MCR ID Level 0", &["MCR ID Level 0"], true).is_none());
    }

    #[test]
    fn mcr_header_matching_is_case_insensitive() {
        let cells = HashMap::from([
            (
                (31, 2),
                SourceCell {
                    value: Data::String("mcr id level 0".into()),
                    formula: None,
                },
            ),
            (
                (31, 3),
                SourceCell {
                    value: Data::String("DATE FORMATE".into()),
                    formula: None,
                },
            ),
        ]);
        let (_, row) = find_header_row(
            &cells,
            "MCR ID Level 0",
            &["MCR ID Level 0", "Date formate"],
            true,
        )
        .expect("case variants should match");
        assert_eq!(row, 31);
    }
}
