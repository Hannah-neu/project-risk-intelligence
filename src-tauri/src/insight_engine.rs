use rusqlite::{params, Connection, Result};

struct GeneratedInsight {
    insight_type: &'static str,
    title: String,
    description: String,
    severity: &'static str,
}

pub fn recalculate_all(connection: &Connection) -> Result<()> {
    let (total, high_critical) = connection.query_row(
        r#"
        SELECT COUNT(*),
          COALESCE(SUM(CASE WHEN risk_level IN ('High', 'Critical') THEN 1 ELSE 0 END), 0)
        FROM project_risks
        "#,
        [],
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
    )?;
    let overdue_late_phase: i64 = connection.query_row(
        r#"SELECT COUNT(*) FROM project_milestones
        WHERE milestone_type IN ('SE3', 'SE4') AND actual_date IS NULL
          AND planned_date < date('now', 'localtime')"#,
        [],
        |row| row.get(0),
    )?;
    let overdue_all: i64 = connection.query_row(
        "SELECT COUNT(*) FROM project_milestones WHERE actual_date IS NULL AND planned_date < date('now', 'localtime')",
        [],
        |row| row.get(0),
    )?;
    let completed_late: i64 = connection.query_row(
        r#"SELECT COUNT(*) FROM project_milestones
        WHERE planned_date IS NOT NULL AND actual_date IS NOT NULL AND actual_date > planned_date"#,
        [],
        |row| row.get(0),
    )?;
    let abnormal_data: i64 = connection.query_row(
        r#"SELECT
          (SELECT COUNT(*) FROM projects WHERE project_status IS NULL OR TRIM(project_status) = '') +
          (SELECT COUNT(*) FROM project_milestones WHERE planned_date IS NULL AND actual_date IS NOT NULL)"#,
        [],
        |row| row.get(0),
    )?;
    let top_category: Option<(String, i64)> = connection
        .query_row(
            r#"SELECT COALESCE(NULLIF(TRIM(p.product_category), ''), 'Uncategorized'), COUNT(*)
            FROM projects p JOIN project_risks r ON r.project_id = p.id
            WHERE r.risk_level IN ('High', 'Critical')
            GROUP BY COALESCE(NULLIF(TRIM(p.product_category), ''), 'Uncategorized')
            ORDER BY COUNT(*) DESC, 1 ASC LIMIT 1"#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .ok();

    let mut insights = vec![
        GeneratedInsight {
            insight_type: "summary",
            title: format!("{high_critical} of {total} projects are High or Critical risk"),
            description: format!("{overdue_all} milestones are overdue and {completed_late} were completed late in the current portfolio."),
            severity: if high_critical > 0 { "high" } else { "low" },
        },
        GeneratedInsight {
            insight_type: "warning",
            title: format!("{high_critical} projects are High or Critical risk"),
            description: "Classification is based on the deterministic risk score for the current dataset.".to_string(),
            severity: if high_critical > 0 { "high" } else { "low" },
        },
        GeneratedInsight {
            insight_type: "milestone",
            title: format!("{overdue_late_phase} overdue SE3 or SE4 milestones"),
            description: format!("{overdue_all} milestones have passed their planned date without an actual date."),
            severity: if overdue_all > 0 { "high" } else { "low" },
        },
        GeneratedInsight {
            insight_type: "recommendation",
            title: format!("{completed_late} milestones were completed late"),
            description: "Delay analysis compares actual completion dates with their planned dates.".to_string(),
            severity: if completed_late > 0 { "medium" } else { "low" },
        },
        GeneratedInsight {
            insight_type: "recommendation",
            title: format!("{abnormal_data} data quality exceptions require review"),
            description: "Checks include missing PRJ Status and actual milestone dates without a planned date.".to_string(),
            severity: if abnormal_data > 0 { "medium" } else { "low" },
        },
    ];
    if let Some((category, projects)) = top_category {
        insights.push(GeneratedInsight {
            insight_type: "portfolio",
            title: format!("Risk is concentrated in {category}"),
            description: format!(
                "{projects} High or Critical projects belong to this product category."
            ),
            severity: "medium",
        });
    } else {
        insights.push(GeneratedInsight {
            insight_type: "portfolio",
            title: "No high-risk category concentration".to_string(),
            description: "No High or Critical project cluster is present in the current dataset."
                .to_string(),
            severity: "low",
        });
    }
    insights.push(GeneratedInsight {
        insight_type: "recommendation",
        title: if overdue_all > 0 { "Prioritize overdue milestone recovery" } else { "Maintain milestone readiness" }.to_string(),
        description: if overdue_all > 0 {
            "Review the highest-risk overdue projects first and record actual completion dates when evidence is available."
        } else {
            "Keep planned and actual milestone dates current so delay analysis remains reliable."
        }.to_string(),
        severity: if overdue_all > 0 { "high" } else { "low" },
    });

    connection.execute("DELETE FROM ai_insights", [])?;
    for insight in insights {
        connection.execute(
            r#"INSERT INTO ai_insights (insight_type, title, description, severity)
            VALUES (?1, ?2, ?3, ?4)"#,
            params![
                insight.insight_type,
                insight.title,
                insight.description,
                insight.severity
            ],
        )?;
    }
    Ok(())
}
