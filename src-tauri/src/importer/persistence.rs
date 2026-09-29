use super::ParsedDataset;
use crate::models::ImportResult;
use chrono::{Local, Utc};
use rusqlite::{params, Connection, Result};

pub fn replace_dataset(
    connection: &mut Connection,
    dataset: &ParsedDataset,
    warning_count: i64,
    formula_error_count: i64,
) -> Result<ImportResult> {
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM ai_insights", [])?;
    transaction.execute("DELETE FROM project_risks", [])?;
    transaction.execute("DELETE FROM project_components", [])?;
    transaction.execute("DELETE FROM project_milestones", [])?;
    transaction.execute("DELETE FROM projects", [])?;

    let mut total_milestones = 0_i64;
    {
        let mut project_statement = transaction.prepare(
            r#"
      INSERT INTO projects (
        project_no, mcr_id, project_name, customer, p_id, acq_manager, project_manager,
        engine_segment, product_category, emission, apic, project_type, system, components,
        new_released_components, has_injector, has_pump, has_rail, has_ecu, has_dnox,
        other_component, project_status, b_sample, c_sample, d_sample, source_file, source_row
      ) VALUES (
        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
        ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27
      )
      "#,
        )?;
        let mut milestone_statement = transaction.prepare(
            r#"
      INSERT INTO project_milestones (
        project_id, milestone_type, planned_date, actual_date, plan_source, actual_source
      ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
      "#,
        )?;
        let mut component_statement = transaction.prepare(
            "INSERT INTO project_components (project_id, component_name, component_type) VALUES (?1, ?2, ?3)",
        )?;

        for project in &dataset.projects {
            project_statement.execute(params![
                project.project_no,
                project.mcr_id,
                project.project_name,
                project.customer,
                project.p_id,
                project.acq_manager,
                project.project_manager,
                project.engine_segment,
                project.product_category,
                project.emission,
                project.apic,
                project.project_type,
                project.system,
                project.components,
                project.new_released_components,
                project.has_injector as i32,
                project.has_pump as i32,
                project.has_rail as i32,
                project.has_ecu as i32,
                project.has_dnox as i32,
                project.other_component,
                project.project_status,
                project.b_sample,
                project.c_sample,
                project.d_sample,
                dataset.source_file,
                project.source_row,
            ])?;
            let project_id = transaction.last_insert_rowid();

            for component in &project.components_list {
                component_statement.execute(params![project_id, component, "existing"])?;
            }
            for component in &project.new_released_components_list {
                component_statement.execute(params![project_id, component, "new_released"])?;
            }

            for milestone in &project.milestones {
                milestone_statement.execute(params![
                    project_id,
                    milestone.milestone_type,
                    milestone.planned_date,
                    milestone.actual_date,
                    milestone.plan_source,
                    milestone.actual_source,
                ])?;
                total_milestones += 1;
            }
        }
    }

    crate::risk_engine::recalculate_all(&transaction)?;
    crate::insight_engine::recalculate_all(&transaction)?;
    crate::risk_engine::mark_cache_current(&transaction, Local::now().date_naive())?;

    let high_risk_projects = transaction.query_row(
        "SELECT COUNT(*) FROM project_risks WHERE risk_level IN ('High', 'Critical')",
        [],
        |row| row.get::<_, i64>(0),
    )?;

    let imported_at = Utc::now().to_rfc3339();
    transaction.execute(
        r#"
    INSERT INTO import_history (
      file_name, import_time, total_projects, total_milestones, status, file_size,
      warning_count, formula_error_count
    ) VALUES (?1, ?2, ?3, ?4, 'success', ?5, ?6, ?7)
    "#,
        params![
            dataset.source_file,
            imported_at,
            dataset.projects.len() as i64,
            total_milestones,
            dataset.file_size as i64,
            warning_count,
            formula_error_count,
        ],
    )?;
    transaction.commit()?;

    Ok(ImportResult {
        file_name: dataset.source_file.clone(),
        total_projects: dataset.projects.len() as i64,
        total_milestones,
        imported_at,
        high_risk_projects,
        warning_count,
        formula_error_count,
    })
}
