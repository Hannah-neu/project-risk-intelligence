use crate::models::{DashboardData, DashboardProject};
use rusqlite::{Connection, Result};

pub(crate) fn metric_definition(metric: &str, zh: bool) -> Option<&'static str> {
    match (metric, zh) {
        ("total", true) => Some("Total Projects = 当前数据库 projects 表中的项目总数；项目列表导入会替换当前数据集。"),
        ("total", false) => Some("Total Projects counts all rows in the current projects table; a Project List import replaces the dataset."),
        ("critical", true) => Some("Critical Projects = risk_level 为 Critical 的项目数。Risk Level 由风险引擎计算，不是导入的 PRJ Status。"),
        ("critical", false) => Some("Critical Projects counts projects whose calculated risk_level is Critical; this is not the imported PRJ Status."),
        ("overdue", true) => Some("Overdue Projects = 至少一个里程碑的计划日期早于本地今天、且实际日期为空的去重项目数。"),
        ("overdue", false) => Some("Overdue Projects counts distinct projects with at least one milestone whose planned date is before local today and whose actual date is null."),
        ("on_track", true) => Some("On Track = 风险级别为 Low 或 Medium、且不存在逾期里程碑的项目数。各 KPI 可重叠，无需相加等于总项目数。"),
        ("on_track", false) => Some("On Track counts projects at Low or Medium risk with no overdue milestone. KPI groups can overlap and need not sum to Total Projects."),
        _ => None,
    }
}

pub fn get(connection: &Connection) -> Result<DashboardData> {
    let total_projects =
        connection.query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))?;
    let (on_track_projects, critical_projects, overdue_projects) = connection.query_row(
        r#"SELECT
              COALESCE(SUM(CASE
                WHEN r.risk_level IN ('Low', 'Medium')
                  AND NOT EXISTS (
                    SELECT 1 FROM project_milestones m
                    WHERE m.project_id = p.id
                      AND DATE(m.planned_date) < DATE('now', 'localtime')
                      AND m.actual_date IS NULL
                  )
                THEN 1 ELSE 0 END), 0),
              COALESCE(SUM(CASE WHEN r.risk_level = 'Critical' THEN 1 ELSE 0 END), 0),
              COALESCE(SUM(CASE WHEN EXISTS (
                SELECT 1 FROM project_milestones m
                WHERE m.project_id = p.id
                  AND DATE(m.planned_date) < DATE('now', 'localtime')
                  AND m.actual_date IS NULL
              ) THEN 1 ELSE 0 END), 0)
            FROM projects p
            LEFT JOIN project_risks r ON r.project_id = p.id"#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let source_file = connection
        .query_row(
            "SELECT file_name FROM import_history WHERE status = 'success' ORDER BY id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    Ok(DashboardData {
        total_projects,
        on_track_projects,
        critical_projects,
        overdue_projects,
        source_file,
        projects: load_projects(connection)?,
        insights: super::insights::dashboard(connection)?,
    })
}

fn load_projects(connection: &Connection) -> Result<Vec<DashboardProject>> {
    let mut statement = connection.prepare(
        r#"
        SELECT
          p.id, p.mcr_id, p.project_name, COALESCE(p.customer, '—'),
          COALESCE(p.project_manager, 'Unassigned'),
          CASE
            WHEN EXISTS (SELECT 1 FROM project_milestones m WHERE m.project_id = p.id AND m.milestone_type = 'QGC4' AND m.actual_date IS NOT NULL) THEN 'Completed'
            WHEN NOT EXISTS (SELECT 1 FROM project_milestones m WHERE m.project_id = p.id AND m.milestone_type = 'SE2' AND m.actual_date IS NOT NULL) THEN 'Early Phase'
            WHEN NOT EXISTS (SELECT 1 FROM project_milestones m WHERE m.project_id = p.id AND m.milestone_type = 'SE3' AND m.actual_date IS NOT NULL) THEN 'SE3 Phase'
            WHEN NOT EXISTS (SELECT 1 FROM project_milestones m WHERE m.project_id = p.id AND m.milestone_type = 'SE4' AND m.actual_date IS NOT NULL) THEN 'SE4 Phase'
            ELSE 'QGC Phase'
          END,
          COALESCE(r.risk_level, 'Low'), COALESCE(r.risk_score, 0),
          COALESCE(NULLIF(UPPER(TRIM(p.project_status)), ''), 'Unspecified'),
          p.updated_at,
          COALESCE(nm.milestone_type, 'Complete'), nm.planned_date
        FROM projects p
        LEFT JOIN project_risks r ON r.project_id = p.id
        LEFT JOIN project_milestones nm ON nm.id = (
          SELECT m.id FROM project_milestones m
          WHERE m.project_id = p.id AND m.actual_date IS NULL
          ORDER BY CASE m.milestone_type
            WHEN 'SE2' THEN 1 WHEN 'SE3' THEN 2 WHEN 'SE4' THEN 3
            WHEN 'QGC0' THEN 4 ELSE 5 END
          LIMIT 1
        )
        ORDER BY COALESCE(r.risk_score, 0) DESC, COALESCE(nm.planned_date, '9999-12-31') ASC
        LIMIT 10
        "#,
    )?;
    let projects = statement.query_map([], |row| {
        Ok(DashboardProject {
            id: row.get(0)?,
            mcr_id: row.get(1)?,
            project_name: row.get(2)?,
            customer: row.get(3)?,
            project_manager: row.get(4)?,
            phase: row.get(5)?,
            risk_level: row.get(6)?,
            risk_score: row.get(7)?,
            project_status: row.get(8)?,
            last_update: row.get(9)?,
            next_milestone: row.get(10)?,
            next_milestone_date: row.get(11)?,
        })
    })?;
    projects.collect()
}
