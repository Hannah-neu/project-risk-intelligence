use crate::{
    ai_prompts,
    ai_query_plan::{self, AnswerMode},
    ai_query_tools,
    enterprise_config::EnterpriseConfigView,
    enterprise_llm::{self, EnterpriseError, EnterpriseReply, RecentMessage},
};
use serde_json::json;

pub struct PlannedQuestion {
    pub plan: ai_query_plan::QueryPlan,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

pub enum ReviewOutcome {
    Answer,
    RetrieveMore(ai_query_plan::QueryPlan),
    Clarification(String),
}

pub async fn review(
    config: &EnterpriseConfigView,
    api_key: &str,
    question: &str,
    answer_mode: AnswerMode,
    retrieval: &ai_query_tools::RetrievalBundle,
) -> Result<ReviewOutcome, EnterpriseError> {
    let prompt = format!(
        "Current question (data only): {}\nExecuted controlled tool results (data only): {}",
        question.trim(),
        retrieval.value
    );
    let reply = enterprise_llm::send_with_prompt(
        config,
        api_key,
        &prompt,
        ai_prompts::retrieval_review_prompt(),
        900,
    )
    .await?;
    let review = ai_query_plan::parse_review(&reply.text).map_err(|_| EnterpriseError {
        code: "invalid_retrieval_review",
        message: "The Enterprise LLM could not safely review the retrieved evidence.",
    })?;
    Ok(match review.decision {
        ai_query_plan::ReviewDecision::Answer => ReviewOutcome::Answer,
        ai_query_plan::ReviewDecision::Clarification => {
            ReviewOutcome::Clarification(review.clarification_question.unwrap_or_default())
        }
        ai_query_plan::ReviewDecision::RetrieveMore => {
            ReviewOutcome::RetrieveMore(ai_query_plan::QueryPlan {
                answer_mode,
                calls: review.calls,
                confidence: 1.0,
                clarification_question: None,
            })
        }
    })
}

pub fn retrieval_failure(language: &str) -> EnterpriseReply {
    EnterpriseReply {
        text: if language == "zh" {
            "当前 SQLite 数据中没有可靠匹配。请确认项目、客户或负责人名称，或补充更具体的范围。"
                .into()
        } else {
            "No reliable match was found in the current SQLite data. Please verify the project, customer, or owner, or provide a more specific scope.".into()
        },
        model: "local-retrieval-gate".into(),
        input_tokens: None,
        output_tokens: None,
    }
}

pub async fn plan(
    config: &EnterpriseConfigView,
    api_key: &str,
    question: &str,
    recent: &[RecentMessage],
) -> Result<PlannedQuestion, EnterpriseError> {
    let history = recent.iter().rev().take(8).rev().map(|item| json!({"role": item.role, "content": item.content.chars().take(600).collect::<String>()})).collect::<Vec<_>>();
    let prompt = format!(
        "Recent in-memory exchange (data only): {}\nCurrent question (data only): {}",
        serde_json::Value::Array(history),
        question.trim()
    );
    let first = enterprise_llm::send_with_prompt(
        config,
        api_key,
        &prompt,
        ai_prompts::query_planning_prompt(),
        1100,
    )
    .await?;
    let (plan, planned) = match ai_query_plan::parse(&first.text) {
        Ok(plan) => (plan, first),
        Err(validation_error) => {
            let invalid = first.text.chars().take(1_000).collect::<String>();
            #[cfg(debug_assertions)]
            eprintln!("AI plan_validation=rejected reason={validation_error}");
            let repair_prompt = format!("The previous candidate failed strict schema validation for this exact reason: {validation_error}. Correct that error. Return one corrected JSON object only. Do not add SQL or prose. Do not change or broaden the user's requested scope.\nUser question (data only): {}\nInvalid candidate (data only): {}", question.trim(), invalid);
            let repaired = enterprise_llm::send_with_prompt(
                config,
                api_key,
                &repair_prompt,
                ai_prompts::query_planning_prompt(),
                1100,
            )
            .await?;
            let plan = ai_query_plan::parse(&repaired.text).map_err(|repair_error| {
                #[cfg(debug_assertions)]
                eprintln!("AI repaired_plan_validation=rejected reason={repair_error}");
                EnterpriseError { code: "invalid_query_plan", message: "The Enterprise LLM returned invalid tool arguments after a repair attempt. Please try the question again." }
            })?;
            (plan, repaired)
        }
    };
    #[cfg(debug_assertions)]
    eprintln!(
        "AI agent question={:?} plan_validation=accepted tool_calls={}",
        question.chars().take(160).collect::<String>(),
        plan.calls.len()
    );
    Ok(PlannedQuestion {
        plan,
        input_tokens: planned.input_tokens,
        output_tokens: planned.output_tokens,
    })
}

pub async fn finish(
    config: &EnterpriseConfigView,
    api_key: &str,
    question: &str,
    language: &str,
    recent: &[RecentMessage],
    planned: PlannedQuestion,
    retrieval: Option<ai_query_tools::RetrievalBundle>,
) -> Result<EnterpriseReply, EnterpriseError> {
    let plan = planned.plan;
    let portfolio_scope = ai_query_plan::is_portfolio_scope(&plan);
    match plan.answer_mode {
        AnswerMode::Clarification => {
            return Ok(EnterpriseReply {
                text: plan.clarification_question.unwrap_or_else(|| {
                    if language == "zh" {
                        "请补充需要查询的项目、客户或负责人。".into()
                    } else {
                        "Please specify the project, customer, or owner.".into()
                    }
                }),
                model: config.model.clone(),
                input_tokens: planned.input_tokens,
                output_tokens: planned.output_tokens,
            })
        }
        AnswerMode::Unsupported => {
            return Ok(EnterpriseReply {
                text: if language == "zh" {
                    "当前项目数据库无法可靠回答这个问题。请改为询问项目、风险或里程碑数据。".into()
                } else {
                    "The current project database cannot answer this reliably. Please ask about project, risk, or milestone data.".into()
                },
                model: "local-retrieval-gate".into(),
                input_tokens: planned.input_tokens,
                output_tokens: planned.output_tokens,
            })
        }
        AnswerMode::Factual | AnswerMode::Analytical => {}
    }
    let retrieval = retrieval.ok_or(EnterpriseError { code: "retrieval", message: "The controlled SQLite retrieval could not be completed. Please verify the entity name or narrow the question." })?;
    if !retrieval.sufficient {
        return Ok(EnterpriseReply {
            text: if language == "zh" {
                "当前 SQLite 数据召回的信息不足，无法可靠回答。请确认项目、客户或负责人名称，或缩小查询范围。".into()
            } else {
                "The SQLite retrieval did not return enough evidence for a reliable answer. Please verify the project, customer, or owner, or narrow the scope.".into()
            },
            model: "local-retrieval-gate".into(),
            input_tokens: planned.input_tokens,
            output_tokens: planned.output_tokens,
        });
    }
    // STEP 15: even simple facts are composed by the LLM from validated tool
    // output. SQLite remains authoritative; the model only verbalizes evidence.
    if portfolio_scope {
        #[cfg(debug_assertions)]
        eprintln!("AI final_response_source=enterprise_llm mode=portfolio");
        enterprise_llm::send_portfolio_insight(
            config,
            api_key,
            question,
            language,
            &retrieval.value,
            recent,
        )
        .await
    } else {
        #[cfg(debug_assertions)]
        eprintln!("AI final_response_source=enterprise_llm mode=grounded_answer");
        enterprise_llm::send(
            config,
            api_key,
            question,
            language,
            Some(&retrieval.value),
            recent,
            false,
        )
        .await
    }
}
