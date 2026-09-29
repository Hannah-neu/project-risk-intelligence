use crate::{
    ai_prompts,
    database::Database,
    enterprise_config::{self, EnterpriseConfigView},
    enterprise_llm::{self, EnterpriseError, EnterpriseReply},
    project_analysis::{self, SavedAnalysis},
};
use rusqlite::Connection;
use tauri::State;

fn error(code: &'static str, message: &'static str) -> EnterpriseError {
    EnterpriseError { code, message }
}

fn configured_provider(
    connection: &Connection,
) -> Result<(EnterpriseConfigView, String), EnterpriseError> {
    let config = enterprise_config::view(connection)
        .map_err(|_| error("settings", "Could not read Enterprise LLM settings."))?;
    if config.provider != "llm" {
        return Err(error(
            "not_configured",
            "Enterprise LLM is not selected. Open AI Configuration, select Enterprise LLM, and save the settings.",
        ));
    }
    if !config.has_api_key {
        return Err(error(
            "not_configured",
            "No Enterprise LLM API key is saved. Open AI Configuration and save a key.",
        ));
    }
    let key = enterprise_config::api_key(connection).map_err(|_| {
        error(
            "not_configured",
            "The Enterprise LLM credential is unavailable. Check AI Configuration.",
        )
    })?;
    Ok((config, key))
}

fn persist_reply(
    connection: &Connection,
    project_id: i64,
    prepared: &project_analysis::PreparedAnalysis,
    model: &str,
    language: &str,
    reply: Result<EnterpriseReply, EnterpriseError>,
) -> Result<SavedAnalysis, EnterpriseError> {
    let reply = reply?;
    project_analysis::save(connection, project_id, prepared, model, language, &reply.text)
        .map_err(|_| error("stale_or_save", "The project changed during analysis or the report could not be saved. Please try again."))
}

#[tauri::command]
pub fn load_project_ai_analysis(
    database: State<'_, Database>,
    project_id: i64,
) -> Result<Option<SavedAnalysis>, String> {
    if project_id <= 0 {
        return Err("Select a valid project.".into());
    }
    let connection = database
        .connection()
        .map_err(|_| "Could not open the local project database.")?;
    project_analysis::latest(&connection, project_id)
}

#[tauri::command]
pub async fn generate_project_ai_analysis(
    database: State<'_, Database>,
    project_id: i64,
    language: String,
) -> Result<SavedAnalysis, EnterpriseError> {
    if project_id <= 0 || (language != "zh" && language != "en") {
        return Err(error(
            "invalid_request",
            "Select a valid project and report language.",
        ));
    }
    let (mut config, api_key, prepared) = {
        let connection = database
            .connection()
            .map_err(|_| error("database", "Could not open the local project database."))?;
        let (config, key) = configured_provider(&connection)?;
        let prepared = project_analysis::prepare(&connection, project_id).map_err(|_| {
            error(
                "context",
                "This project has insufficient current data for AI analysis.",
            )
        })?;
        (config, key, prepared)
    };
    config.model = enterprise_config::DEFAULT_MODEL.to_string();
    let reply = enterprise_llm::send_project_analysis(
        &config,
        &api_key,
        ai_prompts::project_analysis_request(&language),
        &language,
        &prepared.context,
    )
    .await;
    let connection = database
        .connection()
        .map_err(|_| error("database", "Could not save the project analysis locally."))?;
    persist_reply(
        &connection,
        project_id,
        &prepared,
        &config.model,
        &language,
        reply,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::migrations;

    #[test]
    fn unconfigured_provider_is_rejected_before_a_network_request() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        assert_eq!(
            configured_provider(&connection).unwrap_err().code,
            "not_configured"
        );
    }

    #[test]
    fn provider_failure_does_not_create_a_report() {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        connection.execute_batch(r#"
          INSERT INTO projects (id,mcr_id,project_name,source_file,source_row) VALUES (1,'BM-1','Alpha','source.xlsx',2);
          INSERT INTO project_risks (project_id,risk_score,risk_level,risk_reasons) VALUES (1,0,'Low','[]');
        "#).unwrap();
        let prepared = project_analysis::prepare(&connection, 1).unwrap();
        let failure = EnterpriseError {
            code: "timeout",
            message: "The Enterprise LLM request timed out.",
        };
        let result = persist_reply(
            &connection,
            1,
            &prepared,
            "qwen3.6-flash",
            "en",
            Err(failure),
        );
        assert_eq!(result.unwrap_err().code, "timeout");
        assert!(project_analysis::latest(&connection, 1).unwrap().is_none());
    }
}
