use crate::{
    ai_orchestrator, ai_query_tools,
    database::Database,
    enterprise_config::{self, EnterpriseConfigInput, EnterpriseConfigView},
    enterprise_context::{self, RetrievalAssessment, RetrievalStatus},
    enterprise_llm::{self, EnterpriseError, EnterpriseQuestion, EnterpriseReply},
};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub success: bool,
    pub message: &'static str,
}

#[tauri::command]
pub fn get_enterprise_config(
    database: State<'_, Database>,
) -> Result<EnterpriseConfigView, String> {
    let connection = database
        .connection()
        .map_err(|_| "Could not read Enterprise settings.")?;
    enterprise_config::view(&connection)
}

#[tauri::command]
pub fn save_enterprise_config(
    database: State<'_, Database>,
    config: EnterpriseConfigInput,
) -> Result<EnterpriseConfigView, String> {
    let mut connection = database
        .connection()
        .map_err(|_| "Could not save Enterprise settings.")?;
    enterprise_config::save(&mut connection, config)
}

#[tauri::command]
pub async fn invoke_enterprise_llm(
    database: State<'_, Database>,
    request: EnterpriseQuestion,
) -> Result<EnterpriseReply, EnterpriseError> {
    if request.question.trim().is_empty() || request.question.chars().count() > 500 {
        return Err(EnterpriseError {
            code: "invalid_question",
            message: "Enter a question of 1–500 characters.",
        });
    }
    if request.direct_conversation {
        let (config, api_key) = enterprise_credentials(&database)?;
        return enterprise_llm::send_direct(&config, &api_key, &request.question).await;
    }
    if request.structured_query.is_none() {
        let (config, api_key) = enterprise_credentials(&database)?;
        let planned = ai_orchestrator::plan(
            &config,
            &api_key,
            &request.question,
            &request.recent_messages,
        )
        .await?;
        let mut retrieval = if planned.plan.calls.is_empty() {
            None
        } else {
            let connection = database.connection().map_err(|_| EnterpriseError {
                code: "context",
                message: "Could not open the current SQLite dataset for controlled retrieval.",
            })?;
            match ai_query_tools::execute(
                &connection,
                &request.question,
                &request.language,
                &planned.plan,
            ) {
                Ok(retrieval) => Some(retrieval),
                Err(_) => return Ok(ai_orchestrator::retrieval_failure(&request.language)),
            }
        };
        if let Some(current) = retrieval.as_ref() {
            match ai_orchestrator::review(
                &config,
                &api_key,
                &request.question,
                planned.plan.answer_mode,
                current,
            )
            .await
            {
                Ok(ai_orchestrator::ReviewOutcome::RetrieveMore(followup)) => {
                    let connection = database.connection().map_err(|_| EnterpriseError {
                        code: "context",
                        message:
                            "Could not open the current SQLite dataset for follow-up retrieval.",
                    })?;
                    let more = ai_query_tools::execute(
                        &connection,
                        &request.question,
                        &request.language,
                        &followup,
                    )
                    .map_err(|_| EnterpriseError {
                        code: "retrieval",
                        message: "The controlled follow-up retrieval could not be completed.",
                    })?;
                    retrieval = Some(
                        ai_query_tools::merge(retrieval.take().unwrap(), more).map_err(|_| {
                            EnterpriseError {
                                code: "context",
                                message: "The controlled AI context exceeded its safe limit.",
                            }
                        })?,
                    );
                }
                Ok(ai_orchestrator::ReviewOutcome::Clarification(text)) => {
                    return Ok(EnterpriseReply {
                        text,
                        model: config.model.clone(),
                        input_tokens: None,
                        output_tokens: None,
                    })
                }
                Ok(ai_orchestrator::ReviewOutcome::Answer) | Err(_) => {}
            }
        }
        return ai_orchestrator::finish(
            &config,
            &api_key,
            &request.question,
            &request.language,
            &request.recent_messages,
            planned,
            retrieval,
        )
        .await;
    }
    let (config, api_key, context, dataset_version) = {
        let connection = database.connection().map_err(|_| EnterpriseError {
            code: "settings",
            message: "Could not read Enterprise settings.",
        })?;
        let config = enterprise_config::view(&connection).map_err(|_| EnterpriseError {
            code: "settings",
            message: "Could not read Enterprise settings.",
        })?;
        if config.provider != "llm" {
            return Err(EnterpriseError {
                code: "not_configured",
                message: "Enterprise LLM is not selected in Settings.",
            });
        }
        let api_key = enterprise_config::api_key(&connection).map_err(|_| EnterpriseError {
            code: "not_configured",
            message: "Enterprise LLM API key is not configured or could not be unlocked.",
        })?;
        let context =
            enterprise_context::build(&connection, &request).map_err(|_| EnterpriseError {
                code: "context",
                message: "Could not prepare a safe, compact project context for this question.",
            })?;
        let dataset_version =
            crate::qa::dataset_version(&connection).map_err(|_| EnterpriseError {
                code: "context",
                message: "Could not check the current dataset.",
            })?;
        #[cfg(debug_assertions)]
        eprintln!(
            "AI route=llm context_projects={} context_bytes={} model={}",
            context.project_count, context.byte_count, config.model
        );
        (config, api_key, context, dataset_version)
    };
    if context.assessment.status == RetrievalStatus::Insufficient {
        return Ok(insufficient_retrieval_reply(
            &request.language,
            &context.assessment,
        ));
    }
    let recent = if request.dataset_version.as_deref() == Some(dataset_version.as_str()) {
        &request.recent_messages[..]
    } else {
        &[]
    };
    enterprise_llm::send(
        &config,
        &api_key,
        &request.question,
        &request.language,
        Some(&context.value),
        recent,
        false,
    )
    .await
}

fn insufficient_retrieval_reply(
    language: &str,
    assessment: &RetrievalAssessment,
) -> EnterpriseReply {
    let detail = assessment
        .reason
        .as_deref()
        .unwrap_or("The retrieved SQLite evidence is insufficient for a reliable answer.");
    let text = if language == "zh" {
        format!("当前 SQLite 数据中没有检索到足够的信息来可靠回答这个问题（{detail}）。请确认项目、客户或负责人名称，或补充更具体的查询条件。")
    } else {
        format!("The current SQLite data did not provide enough information to answer reliably ({detail}). Please verify the project, customer, or owner name, or add a more specific condition.")
    };
    EnterpriseReply {
        text,
        model: "local-retrieval-gate".into(),
        input_tokens: None,
        output_tokens: None,
    }
}

fn enterprise_credentials(
    database: &State<'_, Database>,
) -> Result<(EnterpriseConfigView, String), EnterpriseError> {
    let connection = database.connection().map_err(|_| EnterpriseError {
        code: "settings",
        message: "Could not read Enterprise settings.",
    })?;
    let config = enterprise_config::view(&connection).map_err(|_| EnterpriseError {
        code: "settings",
        message: "Could not read Enterprise settings.",
    })?;
    if config.provider != "llm" {
        return Err(EnterpriseError {
            code: "not_configured",
            message: "Enterprise LLM is not selected in Settings.",
        });
    }
    let key = enterprise_config::api_key(&connection).map_err(|_| EnterpriseError {
        code: "not_configured",
        message: "Enterprise LLM API key is not configured or could not be unlocked.",
    })?;
    Ok((config, key))
}

#[derive(Debug, serde::Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SemanticIntent {
    FactualQuery,
    ProjectFilter,
    Aggregation,
    AnalyticalQuestion,
    GeneralConversation,
    Unsupported,
}

#[derive(Debug, serde::Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticPlan {
    pub intent: SemanticIntent,
    pub query: Option<crate::qa::structured::StructuredQuery>,
    pub confidence: f64,
}

#[tauri::command]
pub async fn interpret_enterprise_question(
    database: State<'_, Database>,
    question: String,
) -> Result<SemanticPlan, EnterpriseError> {
    if question.trim().is_empty() || question.chars().count() > 500 {
        return Err(EnterpriseError {
            code: "invalid_question",
            message: "Enter a question of 1–500 characters.",
        });
    }
    let (config, api_key) = enterprise_credentials(&database)?;
    let reply = enterprise_llm::send_semantic(&config, &api_key, &question).await?;
    let plan = parse_semantic_plan(&reply.text)?;
    let mut plan = plan;
    if plan.confidence < 0.55 || plan.confidence > 1.0 {
        return Err(EnterpriseError { code: "unsafe_semantic_plan", message: "The question could not be interpreted confidently. Please restate it more specifically." });
    }
    match plan.intent {
        SemanticIntent::GeneralConversation | SemanticIntent::Unsupported => {
            if plan.query.is_some() {
                return Err(EnterpriseError {
                    code: "invalid_semantic_plan",
                    message: "The semantic query plan was invalid.",
                });
            }
        }
        _ => {
            let query = plan.query.as_mut().ok_or(EnterpriseError {
                code: "invalid_semantic_plan",
                message: "The semantic query plan was incomplete.",
            })?;
            if query.filters.is_empty() {
                return Err(EnterpriseError { code: "unsafe_semantic_plan", message: "The interpreted query was too broad. Please add a project, person, customer, status, or risk condition." });
            }
            if plan.intent == SemanticIntent::AnalyticalQuestion
                && (!matches!(query.entity, crate::qa::structured::Entity::Project)
                    || !matches!(query.operation, crate::qa::structured::Operation::List)
                    || query.limit.unwrap_or(8) > 8)
            {
                return Err(EnterpriseError {
                    code: "unsafe_semantic_plan",
                    message: "Analytical questions must use a bounded project selection.",
                });
            }
            let connection = database.connection().map_err(|_| EnterpriseError {
                code: "context",
                message: "Could not validate the semantic query against the current dataset.",
            })?;
            validate_semantic_query(&connection, query)?;
        }
    }
    #[cfg(debug_assertions)]
    eprintln!(
        "AI parser_source=llm_semantic intent={:?} validation=accepted",
        plan.intent
    );
    Ok(plan)
}

fn parse_semantic_plan(text: &str) -> Result<SemanticPlan, EnterpriseError> {
    let without_prefix = text
        .trim()
        .strip_prefix("```json")
        .or_else(|| text.trim().strip_prefix("```"))
        .unwrap_or(text.trim());
    let trimmed = without_prefix
        .strip_suffix("```")
        .unwrap_or(without_prefix)
        .trim();
    let value: serde_json::Value = serde_json::from_str(trimmed).map_err(|_| EnterpriseError {
        code: "invalid_semantic_plan",
        message: "The Enterprise LLM returned an invalid semantic query plan.",
    })?;
    if contains_sql_key(&value) {
        return Err(EnterpriseError {
            code: "unsafe_semantic_plan",
            message: "Raw SQL is not accepted from the Enterprise LLM.",
        });
    }
    serde_json::from_value(value).map_err(|_| EnterpriseError {
        code: "invalid_semantic_plan",
        message: "The Enterprise LLM returned unsupported query fields or operators.",
    })
}

fn contains_sql_key(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Object(map) => map.iter().any(|(key, value)| {
            key.to_ascii_lowercase().contains("sql") || contains_sql_key(value)
        }),
        serde_json::Value::Array(values) => values.iter().any(contains_sql_key),
        _ => false,
    }
}

fn validate_semantic_query(
    connection: &rusqlite::Connection,
    query: &mut crate::qa::structured::StructuredQuery,
) -> Result<(), EnterpriseError> {
    use crate::qa::structured::{Field, Operator};
    if query.filters.len() > 12 || query.limit.is_some_and(|limit| limit == 0 || limit > 50) {
        return Err(EnterpriseError {
            code: "unsafe_semantic_plan",
            message: "The semantic query exceeded the safe query limits.",
        });
    }
    for filter in &mut query.filters {
        if filter.field == Field::ProjectId
            || filter.operator == Operator::In
            || filter.values.is_some()
        {
            return Err(EnterpriseError {
                code: "unsafe_semantic_plan",
                message: "Internal project identifiers cannot be supplied by the Enterprise LLM.",
            });
        }
        let Some(value) = filter
            .value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
        else {
            continue;
        };
        let column = match filter.field {
            Field::Customer => Some(("projects", "customer")),
            Field::Owner | Field::ProjectManager => Some(("projects", "project_manager")),
            Field::PrjStatus => Some(("projects", "project_status")),
            Field::RiskLevel => Some(("project_risks", "risk_level")),
            Field::System => Some(("projects", "system")),
            Field::ProductCategory => Some(("projects", "product_category")),
            Field::McrId => Some(("projects", "mcr_id")),
            Field::ProjectName => Some(("projects", "project_name")),
            Field::Component | Field::NewReleasedComponent => {
                Some(("project_components", "component_name"))
            }
            _ => None,
        };
        if let Some((table, column)) = column {
            let sql = format!("SELECT DISTINCT {column} FROM {table} WHERE {column} IS NOT NULL AND TRIM({column})<>''");
            let mut statement = connection.prepare(&sql).map_err(|_| EnterpriseError {
                code: "context",
                message: "Could not resolve semantic entities against SQLite.",
            })?;
            let candidates = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|_| EnterpriseError {
                    code: "context",
                    message: "Could not resolve semantic entities against SQLite.",
                })?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|_| EnterpriseError {
                    code: "context",
                    message: "Could not resolve semantic entities against SQLite.",
                })?;
            let matched = match filter.operator {
                Operator::PersonNameEquals => candidates.iter().find(|item| {
                    crate::qa::structured::normalize_person_name(item)
                        == crate::qa::structured::normalize_person_name(&value)
                }),
                Operator::SurnameEquals => candidates.iter().find(|item| {
                    crate::qa::structured::normalize_person_name(item)
                        .split_whitespace()
                        .next()
                        == crate::qa::structured::normalize_person_name(&value)
                            .split_whitespace()
                            .next()
                }),
                Operator::Contains => candidates
                    .iter()
                    .find(|item| item.to_lowercase().contains(&value.to_lowercase())),
                Operator::StartsWith => candidates
                    .iter()
                    .find(|item| item.to_lowercase().starts_with(&value.to_lowercase())),
                Operator::Equals => candidates
                    .iter()
                    .find(|item| item.trim().eq_ignore_ascii_case(&value)),
                _ => None,
            };
            if matches!(
                filter.operator,
                Operator::Equals
                    | Operator::Contains
                    | Operator::StartsWith
                    | Operator::PersonNameEquals
                    | Operator::SurnameEquals
            ) {
                let existing = matched.ok_or(EnterpriseError { code: "unknown_entity", message: "An entity in the interpreted question does not exist in the current dataset." })?;
                if filter.operator == Operator::Equals {
                    filter.value = Some(existing.clone());
                }
            }
        }
    }
    crate::qa::structured::answer(connection, query).map_err(|_| EnterpriseError {
        code: "invalid_semantic_plan",
        message: "The interpreted query is not supported by the controlled SQLite query layer.",
    })?;
    Ok(())
}

#[tauri::command]
pub async fn test_enterprise_connection(
    database: State<'_, Database>,
) -> Result<ConnectionTestResult, EnterpriseError> {
    // Use the same provider/key gate as Ask Project Data and Project Analysis.
    // A successful connection test must mean those features can use the provider.
    let (config, api_key) = enterprise_credentials(&database)?;
    enterprise_llm::send(&config, &api_key, "", "en", None, &[], true).await?;
    Ok(ConnectionTestResult {
        success: true,
        message: "Connection successful",
    })
}

#[cfg(test)]
mod semantic_tests {
    use super::*;
    use crate::{
        database::migrations,
        qa::structured::{Entity, Field, Filter, Operation, Operator, StructuredQuery},
    };
    use rusqlite::Connection;

    fn fixture() -> Connection {
        let mut connection = Connection::open_in_memory().unwrap();
        migrations::run(&mut connection).unwrap();
        connection.execute("INSERT INTO projects(id,mcr_id,project_name,customer,project_manager,source_file,source_row) VALUES(1,'BM-1','Alpha','ABC','WU Wei (RBXX/SM3)','x.xlsx',2)", []).unwrap();
        connection
    }

    #[test]
    fn semantic_json_rejects_sql_unknown_fields_and_low_confidence() {
        let raw_sql = r#"{"intent":"factual_query","query":{"entity":"project","operation":"count","filters":[],"sql":"SELECT * FROM projects"},"confidence":0.99}"#;
        assert_eq!(
            parse_semantic_plan(raw_sql).unwrap_err().code,
            "unsafe_semantic_plan"
        );
        let unknown = r#"{"intent":"factual_query","query":{"entity":"project","operation":"count","filters":[{"field":"secret","operator":"equals","value":"x"}]},"confidence":0.99}"#;
        assert_eq!(
            parse_semantic_plan(unknown).unwrap_err().code,
            "invalid_semantic_plan"
        );
    }

    #[test]
    fn semantic_entities_are_checked_against_sqlite_and_internal_ids_are_rejected() {
        let db = fixture();
        let mut known = StructuredQuery {
            entity: Entity::Project,
            operation: Operation::Count,
            target: None,
            filters: vec![Filter {
                field: Field::ProjectManager,
                operator: Operator::PersonNameEquals,
                value: Some("wu wei".into()),
                values: None,
            }],
            sort: None,
            limit: None,
        };
        validate_semantic_query(&db, &mut known).unwrap();
        known.filters[0].value = Some("Invented Person".into());
        assert_eq!(
            validate_semantic_query(&db, &mut known).unwrap_err().code,
            "unknown_entity"
        );
        known.filters[0] = Filter {
            field: Field::ProjectId,
            operator: Operator::In,
            value: None,
            values: Some(vec![1]),
        };
        assert_eq!(
            validate_semantic_query(&db, &mut known).unwrap_err().code,
            "unsafe_semantic_plan"
        );
    }

    #[test]
    fn insufficient_retrieval_reply_is_bilingual_and_does_not_claim_an_answer() {
        let assessment = RetrievalAssessment {
            status: RetrievalStatus::Insufficient,
            total_matches: 0,
            included_records: 0,
            missing_fields: vec![],
            reason: Some("No projects matched the controlled SQLite query.".into()),
        };
        let zh = insufficient_retrieval_reply("zh", &assessment);
        assert_eq!(zh.model, "local-retrieval-gate");
        assert!(zh.text.contains("没有检索到足够的信息"));
        let en = insufficient_retrieval_reply("en", &assessment);
        assert!(en.text.contains("did not provide enough information"));
    }
}
