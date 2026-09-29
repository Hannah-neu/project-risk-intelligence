use crate::models::{
    ProjectDetail, ProjectFilterOptions, ProjectListItem, ProjectListQuery, ProjectListResponse,
    ProjectMilestoneDetail, ProjectRiskDetail,
};
use chrono::Local;
use rusqlite::{params, Connection, OptionalExtension, Result};

pub fn count(connection: &Connection) -> Result<i64> {
    connection.query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))
}

pub fn detail(connection: &Connection, project_id: i64) -> Result<ProjectDetail> {
    let mut project = connection.query_row(
        r#"
        SELECT id, project_no, mcr_id, project_name, customer, p_id, acq_manager,
          project_manager, engine_segment, product_category, emission, apic,
          project_type, system, components, new_released_components,
          has_injector, has_pump, has_rail, has_ecu, has_dnox, other_component,
          CASE WHEN project_status IS NULL OR TRIM(project_status) = '' THEN NULL
            ELSE UPPER(TRIM(project_status)) END,
          b_sample, c_sample, d_sample, source_file, source_row, updated_at
        FROM projects WHERE id = ?1
        "#,
        [project_id],
        |row| {
            Ok(ProjectDetail {
                id: row.get(0)?,
                project_no: row.get(1)?,
                mcr_id: row.get(2)?,
                project_name: row.get(3)?,
                customer: row.get(4)?,
                p_id: row.get(5)?,
                acq_manager: row.get(6)?,
                project_manager: row.get(7)?,
                engine_segment: row.get(8)?,
                product_category: row.get(9)?,
                emission: row.get(10)?,
                apic: row.get(11)?,
                project_type: row.get(12)?,
                system: row.get(13)?,
                components: row.get(14)?,
                new_released_components: row.get(15)?,
                has_injector: row.get::<_, i64>(16)? != 0,
                has_pump: row.get::<_, i64>(17)? != 0,
                has_rail: row.get::<_, i64>(18)? != 0,
                has_ecu: row.get::<_, i64>(19)? != 0,
                has_dnox: row.get::<_, i64>(20)? != 0,
                other_component: row.get(21)?,
                project_status: row.get(22)?,
                b_sample: row.get(23)?,
                c_sample: row.get(24)?,
                d_sample: row.get(25)?,
                source_file: row.get(26)?,
                source_row: row.get(27)?,
                updated_at: row.get(28)?,
                milestones: Vec::new(),
                risk: empty_risk(),
            })
        },
    )?;

    project.risk = connection
        .query_row(
            r#"
            SELECT risk_score, risk_level, primary_risk_driver,
              risk_reasons, calculated_at
            FROM project_risks WHERE project_id = ?1
            "#,
            [project_id],
            |row| {
                let reasons_json: String = row.get(3)?;
                Ok(ProjectRiskDetail {
                    risk_score: row.get(0)?,
                    risk_level: row.get(1)?,
                    primary_risk_driver: row.get(2)?,
                    risk_reasons: serde_json::from_str(&reasons_json).unwrap_or_default(),
                    calculated_at: row.get(4)?,
                })
            },
        )
        .optional()?
        .unwrap_or_else(empty_risk);

    let today = Local::now().date_naive();
    let mut statement = connection.prepare(
        r#"
        SELECT id, milestone_type, planned_date, actual_date
        FROM project_milestones
        WHERE project_id = ?1
        ORDER BY CASE milestone_type
          WHEN 'SE2' THEN 1 WHEN 'SE3' THEN 2 WHEN 'SE4' THEN 3
          WHEN 'QGC0' THEN 4 WHEN 'QGC4' THEN 5 ELSE 6 END
        "#,
    )?;
    project.milestones = statement
        .query_map([project_id], |row| {
            let milestone_type: String = row.get(1)?;
            let planned_date: Option<String> = row.get(2)?;
            let actual_date: Option<String> = row.get(3)?;
            let (status, delta_days) = crate::risk_engine::milestone_display(
                &milestone_type,
                planned_date.as_deref(),
                actual_date.as_deref(),
                today,
            );
            Ok(ProjectMilestoneDetail {
                id: row.get(0)?,
                milestone_type,
                planned_date,
                actual_date,
                delta_days,
                status: status.to_string(),
            })
        })?
        .collect::<Result<Vec<_>>>()?;

    Ok(project)
}

fn empty_risk() -> ProjectRiskDetail {
    ProjectRiskDetail {
        risk_score: 0,
        risk_level: "Low".to_string(),
        primary_risk_driver: None,
        risk_reasons: Vec::new(),
        calculated_at: None,
    }
}

pub fn list(connection: &Connection, query: &ProjectListQuery) -> Result<ProjectListResponse> {
    let search = normalized(&query.search);
    let customer = normalized(&query.customer);
    let project_manager = normalized(&query.project_manager);
    let acq_manager = normalized(&query.acq_manager);
    let engine_segment = normalized(&query.engine_segment);
    let product_category = normalized(&query.product_category);
    let emission = normalized(&query.emission);
    let project_type = normalized(&query.project_type);
    let system = normalized(&query.system);
    let project_status = normalized(&query.project_status).to_uppercase();
    let risk_level = normalized(&query.risk_level);
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);
    let order_column = match query.sort_by.as_deref() {
        Some("mcrId") => "p.mcr_id",
        Some("projectName") => "p.project_name",
        Some("customer") => "p.customer",
        Some("projectManager") => "p.project_manager",
        Some("system") => "p.system",
        Some("productCategory") => "p.product_category",
        Some("projectStatus") => "UPPER(TRIM(p.project_status))",
        Some("nextMilestone") => "nm.planned_date",
        _ => "r.risk_score",
    };
    let order_direction = if query.sort_direction.as_deref() == Some("asc") {
        "ASC"
    } else {
        "DESC"
    };
    let filters_sql = r#"
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
    let arguments = params![
        search,
        customer,
        project_manager,
        acq_manager,
        engine_segment,
        product_category,
        emission,
        project_type,
        system,
        project_status,
        risk_level,
    ];
    let total_sql = format!(
        "SELECT COUNT(*) FROM projects p LEFT JOIN project_risks r ON r.project_id = p.id {filters_sql}"
    );
    let total = connection.query_row(&total_sql, arguments, |row| row.get(0))?;

    let list_sql = format!(
        r#"
        SELECT
          p.id, p.mcr_id, p.project_name, COALESCE(p.customer, '—'),
          COALESCE(p.project_manager, 'Unassigned'), COALESCE(p.system, '—'),
          COALESCE(p.product_category, '—'),
          COALESCE(NULLIF(UPPER(TRIM(p.project_status)), ''), '—'),
          COALESCE(r.risk_level, 'Low'), COALESCE(r.risk_score, 0),
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
        {filters_sql}
        ORDER BY {order_column} {order_direction}, p.project_name ASC
        LIMIT ?12 OFFSET ?13
        "#
    );
    let mut statement = connection.prepare(&list_sql)?;
    let rows = statement.query_map(
        params![
            search,
            customer,
            project_manager,
            acq_manager,
            engine_segment,
            product_category,
            emission,
            project_type,
            system,
            project_status,
            risk_level,
            limit,
            offset,
        ],
        |row| {
            Ok(ProjectListItem {
                id: row.get(0)?,
                mcr_id: row.get(1)?,
                project_name: row.get(2)?,
                customer: row.get(3)?,
                project_manager: row.get(4)?,
                system: row.get(5)?,
                product_category: row.get(6)?,
                project_status: row.get(7)?,
                risk_level: row.get(8)?,
                risk_score: row.get(9)?,
                next_milestone: row.get(10)?,
                next_milestone_date: row.get(11)?,
            })
        },
    )?;

    Ok(ProjectListResponse {
        items: rows.collect::<Result<Vec<_>>>()?,
        total,
        filters: filter_options(connection)?,
    })
}

fn normalized(value: &Option<String>) -> String {
    value.as_deref().unwrap_or_default().trim().to_string()
}

pub(crate) fn filter_options(connection: &Connection) -> Result<ProjectFilterOptions> {
    Ok(ProjectFilterOptions {
        customers: distinct(connection, "projects", "customer")?,
        project_managers: distinct(connection, "projects", "project_manager")?,
        acq_managers: distinct(connection, "projects", "acq_manager")?,
        engine_segments: distinct(connection, "projects", "engine_segment")?,
        product_categories: distinct(connection, "projects", "product_category")?,
        emissions: distinct(connection, "projects", "emission")?,
        project_types: distinct(connection, "projects", "project_type")?,
        systems: distinct(connection, "projects", "system")?,
        project_statuses: distinct_project_statuses(connection)?,
        risk_levels: distinct(connection, "project_risks", "risk_level")?,
    })
}

fn distinct_project_statuses(connection: &Connection) -> Result<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT DISTINCT UPPER(TRIM(project_status)) AS normalized_status
         FROM projects
         WHERE project_status IS NOT NULL AND TRIM(project_status) <> ''
         ORDER BY normalized_status",
    )?;
    let rows = statement.query_map([], |row| row.get(0))?;
    rows.collect()
}

fn distinct(connection: &Connection, table: &str, column: &str) -> Result<Vec<String>> {
    let sql = format!(
        "SELECT DISTINCT {column} FROM {table} WHERE {column} IS NOT NULL AND trim({column}) <> '' ORDER BY {column}"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| row.get(0))?;
    rows.collect()
}
