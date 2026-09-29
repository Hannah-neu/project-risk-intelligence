use crate::models::{
    DashboardInsight, InsightAttentionProject, InsightMilestoneAnalysis, InsightOverviewData,
    InsightPortfolioSummary, InsightRiskDriver, RuleInsight,
};
use rusqlite::{Connection, Result};

pub fn count(connection: &Connection) -> Result<i64> {
    connection.query_row("SELECT COUNT(*) FROM ai_insights", [], |row| row.get(0))
}

pub fn dashboard(connection: &Connection) -> Result<Vec<DashboardInsight>> {
    let mut statement = connection.prepare(
        r#"SELECT id, insight_type, title, description FROM ai_insights
        ORDER BY CASE insight_type WHEN 'warning' THEN 1 WHEN 'milestone' THEN 2
          WHEN 'portfolio' THEN 3 WHEN 'summary' THEN 0 ELSE 4 END, id
        LIMIT 5"#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok(DashboardInsight {
            id: row.get(0)?,
            insight_type: row.get(1)?,
            title: row.get(2)?,
            description: row.get(3)?,
        })
    })?;
    rows.collect()
}

pub fn overview(connection: &Connection) -> Result<InsightOverviewData> {
    Ok(InsightOverviewData {
        generated_at: connection.query_row(
            "SELECT MAX(created_at) FROM ai_insights",
            [],
            |row| row.get(0),
        )?,
        summary: portfolio_summary(connection)?,
        insights: load_insights(connection)?,
        top_risk_drivers: top_risk_drivers(connection)?,
        attention_projects: attention_projects(connection)?,
        milestone_analysis: milestone_analysis(connection)?,
        risk_by_customer: super::risks::breakdown(connection, "customer")?,
        risk_by_project_manager: super::risks::breakdown(connection, "project_manager")?,
    })
}

fn portfolio_summary(connection: &Connection) -> Result<InsightPortfolioSummary> {
    let (total_projects, high_critical_projects) = connection.query_row(
        r#"SELECT COUNT(*),
              COALESCE(SUM(CASE WHEN risk_level IN ('High', 'Critical') THEN 1 ELSE 0 END), 0)
            FROM project_risks"#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let overdue_milestones = connection.query_row(
        "SELECT COUNT(*) FROM project_milestones WHERE actual_date IS NULL AND planned_date < date('now', 'localtime')",
        [],
        |row| row.get(0),
    )?;
    let completed_late_milestones = connection.query_row(
        r#"SELECT COUNT(*) FROM project_milestones
          WHERE actual_date IS NOT NULL AND planned_date IS NOT NULL AND actual_date > planned_date"#,
        [],
        |row| row.get(0),
    )?;
    Ok(InsightPortfolioSummary {
        total_projects,
        high_critical_projects,
        overdue_milestones,
        completed_late_milestones,
    })
}

fn load_insights(connection: &Connection) -> Result<Vec<RuleInsight>> {
    let mut statement = connection.prepare(
        "SELECT id, insight_type, title, description, severity, created_at FROM ai_insights ORDER BY CASE insight_type WHEN 'summary' THEN 1 WHEN 'warning' THEN 2 WHEN 'milestone' THEN 3 WHEN 'portfolio' THEN 4 ELSE 5 END, id",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(RuleInsight {
            id: row.get(0)?,
            insight_type: row.get(1)?,
            title: row.get(2)?,
            description: row.get(3)?,
            severity: row.get(4)?,
            created_at: row.get(5)?,
        })
    })?;
    rows.collect()
}

fn top_risk_drivers(connection: &Connection) -> Result<Vec<InsightRiskDriver>> {
    let mut statement = connection.prepare(
        r#"SELECT CASE
            WHEN lower(primary_risk_driver) LIKE '%overdue%' THEN 'Overdue milestone'
            WHEN lower(primary_risk_driver) LIKE '%completed%late%' THEN 'Completed milestone delay'
            WHEN lower(primary_risk_driver) LIKE '%hold%' THEN 'Project status HOLD'
            ELSE 'Other calculated factor' END AS driver,
          COUNT(*), MAX(risk_score)
        FROM project_risks
        WHERE primary_risk_driver IS NOT NULL
        GROUP BY driver ORDER BY COUNT(*) DESC, MAX(risk_score) DESC LIMIT 6"#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok(InsightRiskDriver {
            driver: row.get(0)?,
            projects: row.get(1)?,
            max_risk_score: row.get(2)?,
        })
    })?;
    rows.collect()
}

fn attention_projects(connection: &Connection) -> Result<Vec<InsightAttentionProject>> {
    let mut statement = connection.prepare(
        r#"SELECT p.id, p.mcr_id, p.project_name, r.risk_score, r.risk_level,
          COALESCE(r.primary_risk_driver, 'No significant driver'),
          COALESCE(nm.milestone_type, 'Complete'), nm.planned_date,
          COALESCE(p.project_manager, 'Unassigned')
        FROM projects p JOIN project_risks r ON r.project_id = p.id
        LEFT JOIN project_milestones nm ON nm.id = (
          SELECT m.id FROM project_milestones m WHERE m.project_id = p.id AND m.actual_date IS NULL
          ORDER BY CASE m.milestone_type WHEN 'SE2' THEN 1 WHEN 'SE3' THEN 2
            WHEN 'SE4' THEN 3 WHEN 'QGC0' THEN 4 ELSE 5 END LIMIT 1)
        WHERE r.risk_level IN ('Critical', 'High')
        ORDER BY r.risk_score DESC, COALESCE(nm.planned_date, '9999-12-31') ASC
        LIMIT 10"#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok(InsightAttentionProject {
            id: row.get(0)?,
            mcr_id: row.get(1)?,
            project_name: row.get(2)?,
            risk_score: row.get(3)?,
            risk_level: row.get(4)?,
            reason: row.get(5)?,
            next_milestone: row.get(6)?,
            next_milestone_date: row.get(7)?,
            owner: row.get(8)?,
        })
    })?;
    rows.collect()
}

fn milestone_analysis(connection: &Connection) -> Result<Vec<InsightMilestoneAnalysis>> {
    let mut statement = connection.prepare(
        r#"SELECT milestone_type,
          SUM(CASE WHEN actual_date IS NULL AND planned_date < date('now', 'localtime') THEN 1 ELSE 0 END),
          SUM(CASE WHEN actual_date IS NOT NULL AND planned_date IS NOT NULL AND actual_date > planned_date THEN 1 ELSE 0 END),
          SUM(CASE WHEN actual_date IS NOT NULL AND planned_date IS NOT NULL AND actual_date <= planned_date THEN 1 ELSE 0 END),
          COALESCE(AVG(CASE WHEN actual_date > planned_date THEN julianday(actual_date) - julianday(planned_date) END), 0)
        FROM project_milestones GROUP BY milestone_type
        ORDER BY CASE milestone_type WHEN 'SE2' THEN 1 WHEN 'SE3' THEN 2 WHEN 'SE4' THEN 3 WHEN 'QGC0' THEN 4 ELSE 5 END"#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok(InsightMilestoneAnalysis {
            milestone: row.get(0)?,
            overdue: row.get(1)?,
            completed_late: row.get(2)?,
            completed_on_time: row.get(3)?,
            average_delay_days: row.get(4)?,
        })
    })?;
    rows.collect()
}
