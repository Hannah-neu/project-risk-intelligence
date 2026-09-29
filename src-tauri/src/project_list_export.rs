use chrono::NaiveDate;
use rusqlite::Connection;
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Cursor, Write},
    path::Path,
};
use zip::{write::SimpleFileOptions, ZipWriter};

#[cfg(test)]
pub const SHEET_NAME: &str = "Active_Project_List";
pub const HEADERS: [&str; 35] = [
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
pub const DATA_SOURCES: [&str; 35] = [
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "Manual",
    "R18_Master_Data",
    "MCR",
    "MCR",
    "MCR",
    "MCR",
    "MCR",
    "AR approval Overview",
    "AR approval Overview",
    "AR approval Overview",
    "OneQ Advanced Report",
    "OneQ Advanced Report",
    "Manual",
    "Manual",
    "Manual",
];
const STAGES: [&str; 5] = ["SE2", "SE3", "SE4", "QGC0", "QGC4"];

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub project_count: usize,
    pub file_path: String,
}

#[derive(Clone, Debug)]
enum Cell {
    Blank,
    Text(String),
    Number(i64),
    Date(NaiveDate),
}

#[derive(Clone, Debug)]
struct ExportProject {
    mcr_id: String,
    cells: Vec<Cell>,
}

pub fn export(connection: &Connection, path: &Path) -> Result<ExportResult, String> {
    validate_target(path)?;
    let projects = load_projects(connection)?;
    validate_projects(&projects)?;
    let bytes = workbook_bytes(&projects)?;
    fs::write(path, bytes).map_err(|error| friendly_write_error(path, &error))?;
    Ok(ExportResult {
        project_count: projects.len(),
        file_path: path.display().to_string(),
    })
}

fn validate_target(path: &Path) -> Result<(), String> {
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_none_or(|value| !value.eq_ignore_ascii_case("xlsx"))
    {
        return Err("Choose a target file with the .xlsx extension.".into());
    }
    let parent = path
        .parent()
        .ok_or("The selected export path is invalid.")?;
    if !parent.is_dir() {
        return Err("The selected export folder does not exist.".into());
    }
    Ok(())
}

fn friendly_write_error(path: &Path, error: &std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        format!("Permission was denied while writing '{}'. Close the workbook if it is open and choose a writable folder.", path.display())
    } else {
        format!("The Excel file could not be written: {error}")
    }
}

fn load_projects(connection: &Connection) -> Result<Vec<ExportProject>, String> {
    let mut statement = connection.prepare(
        "SELECT id,project_no,mcr_id,project_name,customer,p_id,acq_manager,project_manager,engine_segment,product_category,emission,apic,project_type,system,components,new_released_components,has_injector,has_pump,has_rail,has_ecu,has_dnox,other_component,project_status,b_sample,c_sample,d_sample FROM projects ORDER BY CASE WHEN project_no IS NULL THEN 1 ELSE 0 END,project_no,mcr_id COLLATE NOCASE"
    ).map_err(|error| format!("The current project dataset could not be read: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                (4..=15)
                    .map(|index| row.get::<_, Option<String>>(index))
                    .collect::<rusqlite::Result<Vec<_>>>()?,
                (16..=20)
                    .map(|index| row.get::<_, i64>(index).map(|value| value != 0))
                    .collect::<rusqlite::Result<Vec<_>>>()?,
                (21..=25)
                    .map(|index| row.get::<_, Option<String>>(index))
                    .collect::<rusqlite::Result<Vec<_>>>()?,
            ))
        })
        .map_err(|error| format!("The current project dataset could not be read: {error}"))?;
    let mut projects = Vec::new();
    for row in rows {
        let (id, number, mcr_id, name, values, flags, tail) =
            row.map_err(|error| format!("A project row could not be read: {error}"))?;
        let components = component_list(connection, id, "existing", values[10].as_deref())?;
        let new_components = component_list(connection, id, "new_released", values[11].as_deref())?;
        let milestones = milestones(connection, id)?;
        let mut cells = vec![
            number.map(Cell::Number).unwrap_or(Cell::Blank),
            Cell::Text(mcr_id.clone()),
            Cell::Text(name),
        ];
        cells.extend(values[..10].iter().map(optional_text));
        cells.push(optional_text(&components));
        cells.push(optional_text(&new_components));
        cells.extend(flags.into_iter().map(|value| {
            if value {
                Cell::Text("Y".into())
            } else {
                Cell::Blank
            }
        }));
        cells.push(optional_text(&tail[0]));
        cells.push(optional_text(&tail[1]));
        for stage in STAGES {
            cells.push(date_cell(
                milestones.get(stage).and_then(|dates| dates.0.as_deref()),
                &mcr_id,
                &format!("{stage} plan"),
            )?);
        }
        for stage in STAGES {
            cells.push(date_cell(
                milestones.get(stage).and_then(|dates| dates.1.as_deref()),
                &mcr_id,
                &format!("{stage} Act"),
            )?);
        }
        cells.push(date_cell(tail[2].as_deref(), &mcr_id, "B sample")?);
        cells.push(date_cell(tail[3].as_deref(), &mcr_id, "C sample")?);
        cells.push(date_cell(tail[4].as_deref(), &mcr_id, "D sample")?);
        if cells.len() != HEADERS.len() {
            return Err("The Project List export field mapping is incomplete.".into());
        }
        projects.push(ExportProject { mcr_id, cells });
    }
    Ok(projects)
}

fn optional_text(value: &Option<String>) -> Cell {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| Cell::Text(value.to_string()))
        .unwrap_or(Cell::Blank)
}

fn component_list(
    connection: &Connection,
    project_id: i64,
    kind: &str,
    fallback: Option<&str>,
) -> Result<Option<String>, String> {
    let mut statement = connection.prepare("SELECT component_name FROM project_components WHERE project_id=?1 AND component_type=?2 ORDER BY id")
        .map_err(|error| format!("Component data could not be read: {error}"))?;
    let values = statement
        .query_map((project_id, kind), |row| row.get::<_, String>(0))
        .map_err(|error| format!("Component data could not be read: {error}"))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| format!("Component data could not be read: {error}"))?;
    Ok(if values.is_empty() {
        fallback.map(str::to_string)
    } else {
        Some(values.join(", "))
    })
}

fn milestones(
    connection: &Connection,
    project_id: i64,
) -> Result<HashMap<String, (Option<String>, Option<String>)>, String> {
    let mut statement = connection.prepare("SELECT milestone_type,planned_date,actual_date FROM project_milestones WHERE project_id=?1")
        .map_err(|error| format!("Milestone data could not be read: {error}"))?;
    let result = statement
        .query_map([project_id], |row| {
            Ok((row.get(0)?, (row.get(1)?, row.get(2)?)))
        })
        .map_err(|error| format!("Milestone data could not be read: {error}"))?
        .collect::<rusqlite::Result<HashMap<_, _>>>()
        .map_err(|error| format!("Milestone data could not be read: {error}"));
    result
}

fn date_cell(value: Option<&str>, mcr_id: &str, field: &str) -> Result<Cell, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Cell::Blank);
    };
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(Cell::Date)
        .map_err(|_| format!("Project {mcr_id} contains an invalid {field} date: {value}."))
}

fn validate_projects(projects: &[ExportProject]) -> Result<(), String> {
    let mut ids = HashSet::new();
    for project in projects {
        if project.mcr_id.trim().is_empty() {
            return Err("A project is missing the required PRJ MCR-ID.".into());
        }
        if !ids.insert(project.mcr_id.trim().to_ascii_uppercase()) {
            return Err(format!(
                "Duplicate PRJ MCR-ID '{}' blocks export.",
                project.mcr_id
            ));
        }
    }
    Ok(())
}

fn workbook_bytes(projects: &[ExportProject]) -> Result<Vec<u8>, String> {
    let mut sheet = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"2\" topLeftCell=\"A3\" activePane=\"bottomLeft\" state=\"frozen\"/></sheetView></sheetViews><cols><col min=\"1\" max=\"35\" width=\"16\" customWidth=\"1\"/></cols><sheetData>");
    write_string_row(&mut sheet, 1, &DATA_SOURCES, 1);
    write_string_row(&mut sheet, 2, &HEADERS, 2);
    for (index, project) in projects.iter().enumerate() {
        write_data_row(&mut sheet, index as u32 + 3, &project.cells);
    }
    sheet.push_str(&format!(
        "</sheetData><autoFilter ref=\"A2:AI{}\"/></worksheet>",
        projects.len() + 2
    ));

    let cursor = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);
    let options = SimpleFileOptions::default();
    for (name, content) in package_parts(&sheet) {
        zip.start_file(name, options)
            .map_err(|error| format!("Excel generation failed: {error}"))?;
        zip.write_all(content.as_bytes())
            .map_err(|error| format!("Excel generation failed: {error}"))?;
    }
    zip.finish()
        .map(|cursor| cursor.into_inner())
        .map_err(|error| format!("Excel generation failed: {error}"))
}

fn package_parts(sheet: &str) -> Vec<(&'static str, &str)> {
    vec![
        (
            "[Content_Types].xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#,
        ),
        (
            "xl/workbook.xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Active_Project_List" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
        ),
        (
            "xl/_rels/workbook.xml.rels",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#,
        ),
        (
            "xl/styles.xml",
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="164" formatCode="yyyy-mm-dd"/></numFmts><fonts count="2"><font><sz val="11"/><name val="Calibri"/></font><font><b/><sz val="11"/><name val="Calibri"/><color rgb="FFFFFFFF"/></font></fonts><fills count="3"><fill><patternFill patternType="none"/></fill><fill><patternFill patternType="gray125"/></fill><fill><patternFill patternType="solid"><fgColor rgb="FF4472C4"/><bgColor indexed="64"/></patternFill></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="4"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="0" fontId="1" fillId="2" borderId="0" xfId="0" applyFont="1" applyFill="1"/><xf numFmtId="0" fontId="1" fillId="2" borderId="0" xfId="0" applyFont="1" applyFill="1"/><xf numFmtId="164" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#,
        ),
        ("xl/worksheets/sheet1.xml", sheet),
    ]
}

fn write_string_row(sheet: &mut String, row: u32, values: &[&str], style: u8) {
    sheet.push_str(&format!("<row r=\"{row}\">"));
    for (column, value) in values.iter().enumerate() {
        write_text_cell(sheet, &cell_ref(column, row), value, style);
    }
    sheet.push_str("</row>");
}

fn write_data_row(sheet: &mut String, row: u32, cells: &[Cell]) {
    sheet.push_str(&format!("<row r=\"{row}\">"));
    for (column, cell) in cells.iter().enumerate() {
        let reference = cell_ref(column, row);
        match cell {
            Cell::Blank => {}
            Cell::Text(value) => write_text_cell(sheet, &reference, value, 0),
            Cell::Number(value) => {
                sheet.push_str(&format!("<c r=\"{reference}\"><v>{value}</v></c>"))
            }
            Cell::Date(value) => {
                let epoch = NaiveDate::from_ymd_opt(1899, 12, 30).expect("valid Excel epoch");
                let serial = value.signed_duration_since(epoch).num_days();
                sheet.push_str(&format!("<c r=\"{reference}\" s=\"3\"><v>{serial}</v></c>"));
            }
        }
    }
    sheet.push_str("</row>");
}

fn write_text_cell(sheet: &mut String, reference: &str, value: &str, style: u8) {
    let style = if style == 0 {
        String::new()
    } else {
        format!(" s=\"{style}\"")
    };
    sheet.push_str(&format!(
        "<c r=\"{reference}\" t=\"inlineStr\"{style}><is><t xml:space=\"preserve\">{}</t></is></c>",
        xml_escape(value)
    ));
}

fn cell_ref(mut column: usize, row: u32) -> String {
    let mut letters = String::new();
    loop {
        letters.insert(0, (b'A' + (column % 26) as u8) as char);
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    format!("{letters}{row}")
}

fn xml_escape(value: &str) -> String {
    value
        .chars()
        .filter(|character| matches!(*character, '\t' | '\n' | '\r') || *character >= ' ')
        .flat_map(|character| match character {
            '&' => "&amp;".chars().collect::<Vec<_>>(),
            '<' => "&lt;".chars().collect(),
            '>' => "&gt;".chars().collect(),
            '"' => "&quot;".chars().collect(),
            '\'' => "&apos;".chars().collect(),
            value => vec![value],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{database::migrations, importer};
    use calamine::{open_workbook, Data, Reader, Xlsx};
    use rusqlite::params;

    fn fixture() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        migrations::run(&mut db).unwrap();
        db.execute("INSERT INTO projects(id,project_no,mcr_id,project_name,customer,project_manager,components,new_released_components,has_injector,project_status,b_sample,source_file,source_row) VALUES(1,7,'BM-7','Alpha','ABC','Xu Tian','Injector; Pump','ECU',1,'WORK','2026-09-01','source.xlsx',3)", []).unwrap();
        db.execute("INSERT INTO project_components(project_id,component_name,component_type) VALUES(1,'Injector','existing'),(1,'Pump','existing'),(1,'ECU','new_released')", []).unwrap();
        for (stage, plan, actual) in [
            ("SE2", Some("2026-01-02"), Some("2026-01-03")),
            ("SE3", Some("2026-04-05"), None),
        ] {
            db.execute("INSERT INTO project_milestones(project_id,milestone_type,planned_date,actual_date,plan_source,actual_source) VALUES(1,?1,?2,?3,'MCR','Project List')", params![stage,plan,actual]).unwrap();
        }
        db
    }

    fn temp_file(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "project-risk-{label}-{}-{}.xlsx",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap()
        ))
    }

    #[test]
    fn exports_exact_headers_sources_values_dates_and_no_internal_fields() {
        let db = fixture();
        let path = temp_file("mapping");
        export(&db, &path).unwrap();
        let mut workbook: Xlsx<_> = open_workbook(&path).unwrap();
        assert_eq!(workbook.sheet_names(), vec![SHEET_NAME]);
        let range = workbook.worksheet_range(SHEET_NAME).unwrap();
        assert_eq!(
            range
                .rows()
                .next()
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            DATA_SOURCES
        );
        assert_eq!(
            range
                .rows()
                .nth(1)
                .unwrap()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            HEADERS
        );
        let row = range.rows().nth(2).unwrap();
        assert_eq!(row[1].to_string(), "BM-7");
        assert_eq!(row[13].to_string(), "Injector, Pump");
        assert_eq!(row[14].to_string(), "ECU");
        assert!(matches!(row[22], Data::DateTime(_) | Data::DateTimeIso(_)));
        assert!(matches!(row[24], Data::Empty));
        let sheet_xml = {
            let file = std::fs::File::open(&path).unwrap();
            let mut archive = zip::ZipArchive::new(file).unwrap();
            let mut xml = String::new();
            std::io::Read::read_to_string(
                &mut archive.by_name("xl/worksheets/sheet1.xml").unwrap(),
                &mut xml,
            )
            .unwrap();
            xml
        };
        for forbidden in [
            "Risk Score",
            "risk_level",
            "AI Analysis",
            "datasetVersion",
            "source_file",
        ] {
            assert!(!sheet_xml.contains(forbidden));
        }
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn exported_workbook_round_trips_through_current_importer() {
        let db = fixture();
        let path = temp_file("roundtrip");
        export(&db, &path).unwrap();
        let parsed = importer::parse_workbook(&path).unwrap();
        assert_eq!(parsed.projects.len(), 1);
        let project = &parsed.projects[0];
        assert_eq!(project.mcr_id, "BM-7");
        assert_eq!(project.components_list, vec!["Injector", "Pump"]);
        assert_eq!(
            project.milestones[0].planned_date.as_deref(),
            Some("2026-01-02")
        );
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn duplicate_mcr_id_validation_blocks_export() {
        let project = ExportProject {
            mcr_id: "BM-1".into(),
            cells: vec![],
        };
        assert!(validate_projects(&[project.clone(), project])
            .unwrap_err()
            .contains("Duplicate"));
    }
}
