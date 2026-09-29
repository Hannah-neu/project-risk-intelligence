use super::{classifier::ProjectFilter, GroupResult, ProjectResult};
use rusqlite::{params, params_from_iter, types::Value, Connection, Result};

pub fn count_projects(connection: &Connection) -> Result<i64> {
    connection.query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
}

pub fn count_risk_level(connection: &Connection, level: &str) -> Result<i64> {
    connection.query_row(
        "SELECT COUNT(*) FROM project_risks WHERE risk_level = ?1",
        params![level],
        |row| row.get(0),
    )
}

pub fn projects(
    connection: &Connection,
    filter: &ProjectFilter,
) -> Result<(i64, Vec<ProjectResult>)> {
    // Every SQL fragment below is a fixed local constant; user text is only ever bound as a parameter.
    let (predicate, values, limit) = match filter {
        ProjectFilter::Risk(level) => ("r.risk_level = ?1", vec![Value::Text((*level).into())], 50),
        ProjectFilter::Overdue(stage) => (
            "EXISTS (SELECT 1 FROM project_milestones m WHERE m.project_id = p.id AND m.planned_date < date('now', 'localtime') AND m.actual_date IS NULL AND (?1 IS NULL OR m.milestone_type = ?1))",
            vec![stage.map(|value| Value::Text(value.into())).unwrap_or(Value::Null)], 50,
        ),
        ProjectFilter::HighestRisk => ("1 = 1", vec![], 10),
        ProjectFilter::Hold => ("UPPER(TRIM(COALESCE(p.project_status, ''))) = 'HOLD'", vec![], 50),
        ProjectFilter::Customer(value) => ("LOWER(TRIM(COALESCE(p.customer, ''))) = ?1", vec![Value::Text(value.clone())], 50),
        ProjectFilter::Manager(value) => ("LOWER(TRIM(COALESCE(p.project_manager, ''))) = ?1", vec![Value::Text(value.clone())], 50),
        ProjectFilter::CustomerRisk(value, risk) => ("LOWER(TRIM(COALESCE(p.customer, ''))) = ?1 AND r.risk_level = ?2", vec![Value::Text(value.clone()), Value::Text((*risk).into())], 50),
        ProjectFilter::ManagerRisk(value, risk) => ("LOWER(TRIM(COALESCE(p.project_manager, ''))) = ?1 AND r.risk_level = ?2", vec![Value::Text(value.clone()), Value::Text((*risk).into())], 50),
        ProjectFilter::McrId(value) => ("LOWER(TRIM(p.mcr_id)) = ?1", vec![Value::Text(value.clone())], 50),
        ProjectFilter::Ecu => ("p.has_ecu = 1 OR EXISTS (SELECT 1 FROM project_components c WHERE c.project_id = p.id AND LOWER(c.component_name) = 'ecu')", vec![], 50),
        ProjectFilter::NewComponents(component) => (
            "EXISTS (SELECT 1 FROM project_components c WHERE c.project_id = p.id AND c.component_type = 'new_released' AND (?1 IS NULL OR INSTR(LOWER(c.component_name), LOWER(?1)) > 0))",
            vec![component.map(|value| Value::Text(value.into())).unwrap_or(Value::Null)], 50,
        ),
    };
    let from = "FROM projects p JOIN project_risks r ON r.project_id = p.id";
    let total: i64 = connection.query_row(
        &format!("SELECT COUNT(*) {from} WHERE {predicate}"),
        params_from_iter(values.iter()),
        |row| row.get(0),
    )?;
    let mut statement = connection.prepare(&format!(
        "SELECT p.id, p.project_name, p.mcr_id, COALESCE(p.project_status, 'Unspecified'), r.risk_level, r.risk_score, r.primary_risk_driver {from} WHERE {predicate} ORDER BY r.risk_score DESC, p.project_name COLLATE NOCASE, p.id LIMIT {limit}"
    ))?;
    let rows = statement.query_map(params_from_iter(values.iter()), |row| {
        Ok(ProjectResult {
            id: row.get(0)?,
            project_name: row.get(1)?,
            mcr_id: row.get(2)?,
            project_status: row.get(3)?,
            risk_level: row.get(4)?,
            risk_score: row.get(5)?,
            primary_risk_driver: row.get(6)?,
        })
    })?;
    Ok((total, rows.collect::<Result<Vec<_>>>()?))
}

pub fn group_high_risk_customers(connection: &Connection) -> Result<Vec<GroupResult>> {
    groups(connection, "SELECT COALESCE(NULLIF(TRIM(p.customer), ''), 'Unassigned') AS label, COUNT(*) AS n FROM projects p JOIN project_risks r ON r.project_id = p.id WHERE r.risk_level IN ('High', 'Critical') GROUP BY label ORDER BY n DESC, label COLLATE NOCASE LIMIT 20")
}

pub fn group_critical_managers(connection: &Connection) -> Result<Vec<GroupResult>> {
    groups(connection, "SELECT COALESCE(NULLIF(TRIM(p.project_manager), ''), 'Unassigned') AS label, COUNT(*) AS n FROM projects p JOIN project_risks r ON r.project_id = p.id WHERE r.risk_level = 'Critical' GROUP BY label ORDER BY n DESC, label COLLATE NOCASE LIMIT 20")
}

pub fn main_risk_drivers(connection: &Connection) -> Result<Vec<GroupResult>> {
    groups(connection, "SELECT TRIM(primary_risk_driver) AS label, COUNT(*) AS n FROM project_risks WHERE primary_risk_driver IS NOT NULL AND TRIM(primary_risk_driver) <> '' GROUP BY label ORDER BY n DESC, label COLLATE NOCASE LIMIT 10")
}

fn groups(connection: &Connection, sql: &str) -> Result<Vec<GroupResult>> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map([], |row| {
        Ok(GroupResult {
            label: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    rows.collect()
}
