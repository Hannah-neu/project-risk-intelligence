use crate::models::{
    RiskBreakdownItem, RiskBreakdowns, RiskOverviewData, RiskOverviewQuery, RiskTableProject,
};
use rusqlite::{params, Connection, Result};

const REGISTER_FILTERS: &str = r#"
  WHERE (?1 = '' OR lower(
    p.project_name || ' ' || p.mcr_id || ' ' || COALESCE(p.customer, '') || ' ' ||
    COALESCE(p.p_id, '') || ' ' || COALESCE(p.project_manager, '') || ' ' ||
    COALESCE(p.acq_manager, '') || ' ' || COALESCE(p.system, '')
  ) LIKE '%' || lower(?1) || '%')
    AND (?2 = '' OR p.customer = ?2)
    AND (?3 = '' OR p.project_manager = ?3)
    AND (?4 = '' OR p.acq_manager = ?4)
    AND (?5 = '' OR p.engine_segment = ?5)
    AND (?6 = '' OR p.product_category = ?6)
    AND (?7 = '' OR p.emission = ?7)
    AND (?8 = '' OR p.project_type = ?8)
    AND (?9 = '' OR p.system = ?9)
    AND (?10 = '' OR UPPER(TRIM(p.project_status)) = ?10)
    AND (?11 = '' OR r.risk_level = ?11)
"#;

fn normalized(value: &Option<String>) -> String {
    value.as_deref().unwrap_or_default().trim().to_string()
}

fn register_total(connection: &Connection, query: &RiskOverviewQuery) -> Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM projects p JOIN project_risks r ON r.project_id = p.id {REGISTER_FILTERS}");
    connection.query_row(
        &sql,
        params![
            normalized(&query.search),
            normalized(&query.customer),
            normalized(&query.project_manager),
            normalized(&query.acq_manager),
            normalized(&query.engine_segment),
            normalized(&query.product_category),
            normalized(&query.emission),
            normalized(&query.project_type),
            normalized(&query.system),
            normalized(&query.project_status).to_uppercase(),
            normalized(&query.risk_level)
        ],
        |row| row.get(0),
    )
}

pub fn count(connection: &Connection) -> Result<i64> {
    connection.query_row("SELECT COUNT(*) FROM project_risks", [], |row| row.get(0))
}

pub fn overview(connection: &Connection, query: &RiskOverviewQuery) -> Result<RiskOverviewData> {
    let (total_projects, critical, high, medium, low) = connection.query_row(
        r#"
        SELECT COUNT(*),
          COALESCE(SUM(CASE WHEN risk_level = 'Critical' THEN 1 ELSE 0 END), 0),
          COALESCE(SUM(CASE WHEN risk_level = 'High' THEN 1 ELSE 0 END), 0),
          COALESCE(SUM(CASE WHEN risk_level = 'Medium' THEN 1 ELSE 0 END), 0),
          COALESCE(SUM(CASE WHEN risk_level = 'Low' THEN 1 ELSE 0 END), 0)
        FROM project_risks
        "#,
        [],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )?;

    Ok(RiskOverviewData {
        total_projects,
        register_total: register_total(connection, query)?,
        filters: super::projects::filter_options(connection)?,
        critical,
        high,
        medium,
        low,
        breakdowns: RiskBreakdowns {
            customers: breakdown(connection, "customer")?,
            project_managers: breakdown(connection, "project_manager")?,
            systems: breakdown(connection, "system")?,
            product_categories: breakdown(connection, "product_category")?,
        },
        projects: load_projects(connection, query)?,
    })
}

pub(crate) fn breakdown(connection: &Connection, column: &str) -> Result<Vec<RiskBreakdownItem>> {
    let sql = format!(
        r#"
        SELECT COALESCE(NULLIF(TRIM(p.{column}), ''), 'Unassigned') AS label,
          SUM(CASE WHEN r.risk_level = 'Critical' THEN 1 ELSE 0 END),
          SUM(CASE WHEN r.risk_level = 'High' THEN 1 ELSE 0 END),
          SUM(CASE WHEN r.risk_level = 'Medium' THEN 1 ELSE 0 END),
          SUM(CASE WHEN r.risk_level = 'Low' THEN 1 ELSE 0 END),
          COUNT(*), SUM(r.risk_score) AS exposure
        FROM projects p JOIN project_risks r ON r.project_id = p.id
        GROUP BY COALESCE(NULLIF(TRIM(p.{column}), ''), 'Unassigned')
        ORDER BY exposure DESC, COUNT(*) DESC, label ASC
        LIMIT 8
        "#
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        Ok(RiskBreakdownItem {
            label: row.get(0)?,
            critical: row.get(1)?,
            high: row.get(2)?,
            medium: row.get(3)?,
            low: row.get(4)?,
            total: row.get(5)?,
            exposure: row.get(6)?,
        })
    })?;
    rows.collect()
}

fn load_projects(
    connection: &Connection,
    query: &RiskOverviewQuery,
) -> Result<Vec<RiskTableProject>> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);
    let sql = format!(
        r#"
        SELECT p.id, p.mcr_id, p.project_name, COALESCE(p.customer, '—'),
          COALESCE(p.project_manager, 'Unassigned'), r.risk_score, r.risk_level,
          r.primary_risk_driver,
          COALESCE(nm.milestone_type, 'Complete'), nm.planned_date
        FROM projects p
        JOIN project_risks r ON r.project_id = p.id
        LEFT JOIN project_milestones nm ON nm.id = (
          SELECT m.id FROM project_milestones m
          WHERE m.project_id = p.id AND m.actual_date IS NULL
          ORDER BY CASE m.milestone_type
            WHEN 'SE2' THEN 1 WHEN 'SE3' THEN 2 WHEN 'SE4' THEN 3
            WHEN 'QGC0' THEN 4 ELSE 5 END
          LIMIT 1
        )
        {REGISTER_FILTERS}
        ORDER BY r.risk_score DESC, COALESCE(nm.planned_date, '9999-12-31') ASC,
          p.project_name ASC
        LIMIT ?12 OFFSET ?13
        "#,
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            normalized(&query.search),
            normalized(&query.customer),
            normalized(&query.project_manager),
            normalized(&query.acq_manager),
            normalized(&query.engine_segment),
            normalized(&query.product_category),
            normalized(&query.emission),
            normalized(&query.project_type),
            normalized(&query.system),
            normalized(&query.project_status).to_uppercase(),
            normalized(&query.risk_level),
            limit,
            offset
        ],
        |row| {
            Ok(RiskTableProject {
                id: row.get(0)?,
                mcr_id: row.get(1)?,
                project_name: row.get(2)?,
                customer: row.get(3)?,
                project_manager: row.get(4)?,
                risk_score: row.get(5)?,
                risk_level: row.get(6)?,
                primary_risk_driver: row.get(7)?,
                next_milestone: row.get(8)?,
                next_milestone_date: row.get(9)?,
            })
        },
    )?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_filters_count_and_page_without_changing_portfolio_metrics() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(r#"
            CREATE TABLE projects (
              id INTEGER PRIMARY KEY, mcr_id TEXT, project_name TEXT, customer TEXT,
              p_id TEXT, acq_manager TEXT, project_manager TEXT, engine_segment TEXT,
              product_category TEXT, emission TEXT, project_type TEXT, system TEXT,
              project_status TEXT
            );
            CREATE TABLE project_risks (
              project_id INTEGER, risk_score INTEGER, risk_level TEXT, primary_risk_driver TEXT
            );
            CREATE TABLE project_milestones (
              id INTEGER PRIMARY KEY, project_id INTEGER, milestone_type TEXT,
              planned_date TEXT, actual_date TEXT
            );
            INSERT INTO projects VALUES
              (1,'M1','Alpha','Customer A','P1','Acq A','Manager A','Diesel','Engine','EU6','A','System A','work'),
              (2,'M2','Beta','Customer A','P2','Acq B','Manager B','Diesel','Engine','EU6','A','System B','HOLD'),
              (3,'M3','Gamma','Customer B','P3','Acq A','Manager A','Gas','Rail','EU7','K','System A','WORK');
            INSERT INTO project_risks VALUES
              (1,90,'Critical','Overdue SE3'),
              (2,62,'High','Project status is HOLD'),
              (3,17,'Low',NULL);
        "#).unwrap();
        let query = RiskOverviewQuery {
            customer: Some("Customer A".into()),
            project_manager: Some("Manager A".into()),
            risk_level: Some("Critical".into()),
            limit: Some(20),
            ..RiskOverviewQuery::default()
        };
        let result = overview(&connection, &query).unwrap();
        assert_eq!(result.total_projects, 3);
        assert_eq!(result.critical, 1);
        assert_eq!(result.register_total, 1);
        assert_eq!(result.projects.len(), 1);
        assert_eq!(result.projects[0].mcr_id, "M1");
        assert_eq!(result.filters.customers, vec!["Customer A", "Customer B"]);

        let empty_page = overview(
            &connection,
            &RiskOverviewQuery {
                offset: Some(20),
                ..query
            },
        )
        .unwrap();
        assert_eq!(empty_page.register_total, 1);
        assert!(empty_page.projects.is_empty());

        let status = overview(
            &connection,
            &RiskOverviewQuery {
                project_status: Some("work".into()),
                ..RiskOverviewQuery::default()
            },
        )
        .unwrap();
        assert_eq!(status.register_total, 2);
    }
}
