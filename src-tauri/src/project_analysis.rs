use crate::{qa, repositories::projects, risk_engine};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Map, Value};

const MAX_CONTEXT_BYTES: usize = 12_000;

#[derive(Debug)]
pub struct PreparedAnalysis {
    pub context: Value,
    pub dataset_version: String,
    pub project_mcr_id: String,
    pub risk_score: i64,
    pub risk_level: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedAnalysis {
    pub id: i64,
    pub project_id: i64,
    pub project_mcr_id: String,
    pub dataset_version: String,
    pub model: String,
    pub language: String,
    pub content: String,
    pub risk_score_at_generation: i64,
    pub risk_level_at_generation: String,
    pub created_at: String,
    pub outdated: bool,
}

fn add_text(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

pub fn prepare(connection: &Connection, project_id: i64) -> Result<PreparedAnalysis, String> {
    let project = projects::detail(connection, project_id)
        .map_err(|_| "This project is no longer available in the current dataset.".to_string())?;
    let has_risk: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM project_risks WHERE project_id = ?1)",
            [project_id],
            |row| row.get(0),
        )
        .map_err(|_| "Could not verify the project's risk assessment.".to_string())?;
    if !has_risk {
        return Err("This project has no calculated risk assessment yet.".into());
    }

    let mut identity = Map::new();
    add_text(&mut identity, "projectName", Some(&project.project_name));
    add_text(&mut identity, "mcrId", Some(&project.mcr_id));
    add_text(&mut identity, "customer", project.customer.as_deref());
    add_text(
        &mut identity,
        "projectManager",
        project.project_manager.as_deref(),
    );
    add_text(
        &mut identity,
        "prjStatus",
        project.project_status.as_deref(),
    );
    add_text(
        &mut identity,
        "productCategory",
        project.product_category.as_deref(),
    );
    add_text(&mut identity, "system", project.system.as_deref());

    let milestones: Vec<Value> = project
        .milestones
        .iter()
        .map(|milestone| {
            let mut item = Map::new();
            add_text(&mut item, "stage", Some(&milestone.milestone_type));
            add_text(&mut item, "plannedDate", milestone.planned_date.as_deref());
            add_text(&mut item, "actualDate", milestone.actual_date.as_deref());
            add_text(&mut item, "status", Some(&milestone.status));
            if let Some(days) = milestone.delta_days {
                item.insert("deltaDays".into(), json!(days));
            }
            Value::Object(item)
        })
        .collect();

    let mut scope = Map::new();
    add_text(&mut scope, "components", project.components.as_deref());
    add_text(
        &mut scope,
        "newReleasedComponents",
        project.new_released_components.as_deref(),
    );
    let mut samples = Map::new();
    add_text(&mut samples, "bSample", project.b_sample.as_deref());
    add_text(&mut samples, "cSample", project.c_sample.as_deref());
    add_text(&mut samples, "dSample", project.d_sample.as_deref());

    let mut context = Map::new();
    context.insert("scope".into(), json!("singleSelectedProject"));
    context.insert("project".into(), Value::Object(identity));
    context.insert(
        "risk".into(),
        json!({
            "score": project.risk.risk_score,
            "level": project.risk.risk_level,
            "primaryDriver": project.risk.primary_risk_driver,
            "reasons": project.risk.risk_reasons
        }),
    );
    context.insert("milestones".into(), Value::Array(milestones));
    if !scope.is_empty() {
        context.insert("scopeDetails".into(), Value::Object(scope));
    }
    if !samples.is_empty() {
        context.insert("samples".into(), Value::Object(samples));
    }
    context.insert("riskMethodology".into(), risk_engine::methodology_context());
    let context = Value::Object(context);
    let bytes = serde_json::to_vec(&context)
        .map_err(|_| "Could not prepare the project analysis context.".to_string())?
        .len();
    if bytes > MAX_CONTEXT_BYTES {
        return Err("The selected project has too much detail for a compact analysis. Review its scope data first.".into());
    }
    Ok(PreparedAnalysis {
        context,
        dataset_version: qa::dataset_version(connection)
            .map_err(|_| "Could not check the current dataset version.")?,
        project_mcr_id: project.mcr_id,
        risk_score: project.risk.risk_score,
        risk_level: project.risk.risk_level,
    })
}

pub fn latest(connection: &Connection, project_id: i64) -> Result<Option<SavedAnalysis>, String> {
    let mcr_id: Option<String> = connection
        .query_row(
            "SELECT mcr_id FROM projects WHERE id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "Could not load the current project.".to_string())?;
    let Some(mcr_id) = mcr_id else {
        return Ok(None);
    };
    let version = qa::dataset_version(connection)
        .map_err(|_| "Could not check the current dataset version.")?;
    connection.query_row(
        "SELECT id, project_id, project_mcr_id, dataset_version, model, language, content, risk_score_at_generation, risk_level_at_generation, created_at \
         FROM project_ai_analysis WHERE project_mcr_id = ?1 ORDER BY id DESC LIMIT 1",
        [mcr_id], |row| Ok(SavedAnalysis {
            id: row.get(0)?, project_id: row.get(1)?, project_mcr_id: row.get(2)?,
            dataset_version: row.get(3)?, model: row.get(4)?, language: row.get(5)?,
            content: row.get(6)?, risk_score_at_generation: row.get(7)?,
            risk_level_at_generation: row.get(8)?, created_at: row.get(9)?, outdated: false
        })
    ).optional().map(|report| report.map(|mut report| {
        report.outdated = report.dataset_version != version;
        report
    })).map_err(|_| "Could not load the saved project analysis.".to_string())
}

pub fn save(
    connection: &Connection,
    project_id: i64,
    prepared: &PreparedAnalysis,
    model: &str,
    language: &str,
    content: &str,
) -> Result<SavedAnalysis, String> {
    if content.trim().is_empty() {
        return Err("The Enterprise LLM returned an empty analysis.".into());
    }
    let current = prepare(connection, project_id)?;
    if current.dataset_version != prepared.dataset_version
        || current.project_mcr_id != prepared.project_mcr_id
        || current.risk_score != prepared.risk_score
        || current.risk_level != prepared.risk_level
    {
        return Err(
            "The project dataset changed during analysis. Generate the report again.".into(),
        );
    }
    connection.execute(
        "INSERT INTO project_ai_analysis (project_id, project_mcr_id, dataset_version, model, language, content, risk_score_at_generation, risk_level_at_generation) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![project_id, prepared.project_mcr_id, prepared.dataset_version, model, language, content.trim(), prepared.risk_score, prepared.risk_level]
    ).map_err(|_| "Could not save the project analysis locally.".to_string())?;
    latest(connection, project_id)?
        .ok_or_else(|| "Could not load the saved project analysis.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    fn fixture() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        connection.execute_batch(r#"
          INSERT INTO projects (id,mcr_id,project_name,customer,project_manager,project_status,components,new_released_components,b_sample,source_file,source_row)
          VALUES (1,'BM-1','Critical Project','Customer A','Alice','WORK','ECU','Pump',NULL,'source.xlsx',2),
                 (2,'BM-2','Low Project','Customer B','Bob','PLAN',NULL,NULL,NULL,'source.xlsx',3),
                 (3,'BM-3','Normal Project','Customer C','Casey','WORK','Rail',NULL,'B-OK','source.xlsx',4);
          INSERT INTO project_risks (project_id,risk_score,risk_level,primary_risk_driver,risk_reasons)
          VALUES (1,85,'Critical','SE3 overdue','["SE3 overdue"]'),(2,0,'Low',NULL,'[]'),
                 (3,30,'Medium','SE2 completed 31 days late','["SE2 completed 31 days late"]');
          INSERT INTO project_milestones (project_id,milestone_type,planned_date,actual_date,plan_source,actual_source)
          VALUES (1,'SE3','2024-01-01',NULL,'Project List','Project List'),
                 (2,'SE3',NULL,NULL,'Project List','Project List'),
                 (3,'SE2','2024-02-01','2024-03-03','Project List','Project List');
          INSERT INTO import_history (id,file_name,status,total_projects,total_milestones) VALUES (1,'source.xlsx','success',3,3);
        "#).unwrap();
        connection
    }

    #[test]
    fn context_contains_only_selected_project_and_central_risk_rules() {
        let connection = fixture();
        let critical = prepare(&connection, 1).unwrap();
        let serialized = critical.context.to_string();
        assert!(serialized.contains("Critical Project"));
        assert!(!serialized.contains("Low Project"));
        assert!(!serialized.contains("source.xlsx"));
        assert_eq!(critical.context["risk"]["score"], 85);
        assert_eq!(
            critical.context["riskMethodology"]["score"]["maximum"],
            risk_engine::MAX_RISK_SCORE
        );
        assert!(critical.context["milestones"][0]["status"]
            .as_str()
            .unwrap()
            .contains("Overdue"));
        let low = prepare(&connection, 2).unwrap();
        assert_eq!(low.context["risk"]["level"], "Low");
        assert_eq!(low.context["milestones"][0]["status"], "Not planned");
        assert!(low.context.get("scopeDetails").is_none());
        let normal = prepare(&connection, 3).unwrap();
        assert_eq!(normal.context["risk"]["level"], "Medium");
        assert_eq!(normal.context["milestones"][0]["status"], "Completed late");
        assert_eq!(normal.context["milestones"][0]["deltaDays"], 31);
        assert_eq!(normal.context["samples"]["bSample"], "B-OK");
    }

    #[test]
    fn saved_reports_load_and_become_outdated_after_mcr_or_project_list_update() {
        let connection = fixture();
        assert!(latest(&connection, 1).unwrap().is_none());
        let prepared = prepare(&connection, 1).unwrap();
        let first = save(
            &connection,
            1,
            &prepared,
            "qwen3.6-flash",
            "en",
            "1. Overall Assessment\nReview the risk.",
        )
        .unwrap();
        assert!(!first.outdated);
        assert_eq!(latest(&connection, 1).unwrap().unwrap().id, first.id);
        connection.execute("INSERT INTO mcr_sync_history (file_name,file_size,synced_at,master_ids,matched_projects,updated_milestones,projects_affected,warning_count) VALUES ('mcr.xlsx',1,'2026-01-01',1,1,1,1,0)", []).unwrap();
        assert!(latest(&connection, 1).unwrap().unwrap().outdated);
        assert!(save(
            &connection,
            1,
            &prepared,
            "qwen3.6-flash",
            "en",
            "stale reply"
        )
        .is_err());
        let refreshed = prepare(&connection, 1).unwrap();
        let next = save(
            &connection,
            1,
            &refreshed,
            "qwen3.6-flash",
            "zh",
            "1. 总体评估\n请审查。",
        )
        .unwrap();
        assert!(next.id > first.id);
        assert!(!next.outdated);
        connection.execute("INSERT INTO import_history (id,file_name,status,total_projects,total_milestones) VALUES (2,'source2.xlsx','success',2,2)", []).unwrap();
        assert!(latest(&connection, 1).unwrap().unwrap().outdated);
        connection
            .execute("DELETE FROM projects WHERE id = 1", [])
            .unwrap();
        connection.execute("INSERT INTO projects (id,mcr_id,project_name,source_file,source_row) VALUES (10,'BM-1','Replacement','source2.xlsx',2)", []).unwrap();
        let retained = latest(&connection, 10).unwrap().unwrap();
        assert_eq!(retained.id, next.id);
        assert!(retained.outdated);
    }

    #[test]
    fn missing_project_or_risk_and_empty_reply_are_rejected() {
        let connection = fixture();
        assert!(prepare(&connection, 99).is_err());
        connection
            .execute("DELETE FROM project_risks WHERE project_id = 2", [])
            .unwrap();
        assert!(prepare(&connection, 2).is_err());
        let prepared = prepare(&connection, 1).unwrap();
        assert!(save(&connection, 1, &prepared, "qwen3.6-flash", "en", " ").is_err());
        assert!(crate::ai_prompts::project_analysis_request("zh").contains("总体评估"));
        assert!(crate::ai_prompts::project_analysis_request("en").contains("Overall Assessment"));
        assert!(
            crate::ai_prompts::project_analysis_request("zh")
                .chars()
                .count()
                <= 500
        );
        assert!(
            crate::ai_prompts::project_analysis_request("en")
                .chars()
                .count()
                <= 500
        );
    }
}
