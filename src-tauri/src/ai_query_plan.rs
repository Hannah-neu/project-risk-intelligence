use crate::qa::structured::{Field, Operation, StructuredQuery};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_TOOL_CALLS: usize = 4;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnswerMode {
    Factual,
    Analytical,
    Clarification,
    Unsupported,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolName {
    SearchProjects,
    QueryProjects,
    AggregateProjects,
    GetProjectDetails,
    GetRiskEvidence,
    GetMilestoneData,
    ResolveEntity,
    GetBusinessSchema,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ToolCall {
    pub tool: ToolName,
    pub query: Option<StructuredQuery>,
    pub entity_field: Option<Field>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct QueryPlan {
    pub answer_mode: AnswerMode,
    #[serde(default)]
    pub calls: Vec<ToolCall>,
    pub confidence: f64,
    pub clarification_question: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Answer,
    RetrieveMore,
    Clarification,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RetrievalReview {
    pub decision: ReviewDecision,
    #[serde(default)]
    pub calls: Vec<ToolCall>,
    pub clarification_question: Option<String>,
}

pub fn parse_review(text: &str) -> Result<RetrievalReview, &'static str> {
    let without_prefix = text
        .trim()
        .strip_prefix("```json")
        .or_else(|| text.trim().strip_prefix("```"))
        .unwrap_or(text.trim());
    let trimmed = without_prefix
        .strip_suffix("```")
        .unwrap_or(without_prefix)
        .trim();
    let value: Value =
        serde_json::from_str(trimmed).map_err(|_| "invalid JSON retrieval review")?;
    if contains_sql_key(&value) {
        return Err("raw SQL is not accepted");
    }
    let review: RetrievalReview =
        serde_json::from_value(value).map_err(|_| "unsupported retrieval review schema")?;
    if review.calls.len() > 3 {
        return Err("too many follow-up retrieval calls");
    }
    match review.decision {
        ReviewDecision::Answer
            if !review.calls.is_empty() || review.clarification_question.is_some() =>
        {
            return Err("answer review cannot include calls or clarification")
        }
        ReviewDecision::RetrieveMore
            if review.calls.is_empty() || review.clarification_question.is_some() =>
        {
            return Err("invalid follow-up retrieval review")
        }
        ReviewDecision::Clarification
            if !review.calls.is_empty()
                || review
                    .clarification_question
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty() =>
        {
            return Err("invalid clarification review")
        }
        _ => {}
    }
    for call in &review.calls {
        validate_call(call)?;
    }
    Ok(review)
}

pub fn parse(text: &str) -> Result<QueryPlan, &'static str> {
    let without_prefix = text
        .trim()
        .strip_prefix("```json")
        .or_else(|| text.trim().strip_prefix("```"))
        .unwrap_or(text.trim());
    let trimmed = without_prefix
        .strip_suffix("```")
        .unwrap_or(without_prefix)
        .trim();
    let value: Value = serde_json::from_str(trimmed).map_err(|_| "invalid JSON query plan")?;
    if contains_sql_key(&value) {
        return Err("raw SQL is not accepted");
    }
    let plan: QueryPlan =
        serde_json::from_value(value).map_err(|_| "unsupported query plan schema")?;
    validate(&plan)?;
    Ok(plan)
}

pub fn is_portfolio_scope(plan: &QueryPlan) -> bool {
    plan.answer_mode == AnswerMode::Analytical
        && plan.calls.iter().any(|call| {
            call.query.as_ref().is_some_and(|query| {
                query.filters.is_empty()
                    && matches!(
                        call.tool,
                        ToolName::AggregateProjects
                            | ToolName::GetRiskEvidence
                            | ToolName::QueryProjects
                    )
            })
        })
}

pub fn validate(plan: &QueryPlan) -> Result<(), &'static str> {
    if !(0.55..=1.0).contains(&plan.confidence) {
        return Err("query plan confidence is too low");
    }
    if plan.calls.len() > MAX_TOOL_CALLS {
        return Err("too many retrieval calls");
    }
    match plan.answer_mode {
        AnswerMode::Clarification
            if plan
                .clarification_question
                .as_deref()
                .unwrap_or("")
                .trim()
                .is_empty()
                || !plan.calls.is_empty() =>
        {
            return Err("invalid clarification plan")
        }
        AnswerMode::Unsupported if !plan.calls.is_empty() => {
            return Err("unsupported plans cannot run tools")
        }
        AnswerMode::Factual | AnswerMode::Analytical if plan.calls.is_empty() => {
            return Err("retrieval plan has no tool calls")
        }
        _ => {}
    }
    for call in &plan.calls {
        validate_call(call)?;
    }
    Ok(())
}

fn validate_call(call: &ToolCall) -> Result<(), &'static str> {
    if call.tool == ToolName::GetBusinessSchema {
        if call.query.is_some() || call.entity_field.is_some() || call.value.is_some() {
            return Err("business schema tool does not accept arguments");
        }
        return Ok(());
    }
    if call.tool == ToolName::SearchProjects {
        if call.query.is_some() {
            return Err(
                "search_projects requires query:null and accepts only entityField plus value",
            );
        }
        if call.value.as_deref().unwrap_or("").trim().is_empty() {
            return Err("search_projects requires a non-empty value");
        }
        if call.entity_field.is_some_and(|field| {
            !matches!(
                field,
                Field::Customer
                    | Field::Owner
                    | Field::ProjectManager
                    | Field::PrjStatus
                    | Field::System
                    | Field::ProductCategory
                    | Field::McrId
                    | Field::ProjectName
                    | Field::Component
                    | Field::NewReleasedComponent
            )
        }) {
            return Err("project search field is not whitelisted");
        }
        return Ok(());
    }
    if call.tool == ToolName::ResolveEntity {
        if call.query.is_some() || call.value.as_deref().unwrap_or("").trim().is_empty() {
            return Err("invalid entity resolution call");
        }
        if !matches!(
            call.entity_field,
            Some(
                Field::Customer
                    | Field::Owner
                    | Field::ProjectManager
                    | Field::PrjStatus
                    | Field::RiskLevel
                    | Field::System
                    | Field::ProductCategory
                    | Field::McrId
                    | Field::ProjectName
                    | Field::Component
                    | Field::NewReleasedComponent
            )
        ) {
            return Err("entity field is not whitelisted");
        }
        return Ok(());
    }
    if call.entity_field.is_some() || call.value.is_some() {
        return Err("query tools cannot include resolver arguments");
    }
    let query = call
        .query
        .as_ref()
        .ok_or("query tool has no structured query")?;
    if query.filters.len() > 12 || query.limit.is_some_and(|limit| limit == 0 || limit > 50) {
        return Err("query exceeds safe limits");
    }
    if query
        .filters
        .iter()
        .any(|filter| filter.field == Field::ProjectId || filter.values.is_some())
    {
        return Err("internal project identifiers are not accepted from the planner");
    }
    let operation_ok = match call.tool {
        ToolName::QueryProjects => matches!(
            query.operation,
            Operation::List | Operation::Top | Operation::Sort
        ),
        ToolName::AggregateProjects => matches!(
            query.operation,
            Operation::Count
                | Operation::CountDistinct
                | Operation::Distinct
                | Operation::GroupBy
                | Operation::Max
                | Operation::Min
        ),
        ToolName::GetProjectDetails => {
            matches!(query.operation, Operation::List)
                && query
                    .filters
                    .iter()
                    .any(|filter| matches!(filter.field, Field::McrId | Field::ProjectName))
        }
        ToolName::GetRiskEvidence | ToolName::GetMilestoneData => matches!(
            query.operation,
            Operation::List | Operation::Top | Operation::Sort
        ),
        ToolName::ResolveEntity => unreachable!(),
        ToolName::SearchProjects | ToolName::GetBusinessSchema => unreachable!(),
    };
    if !operation_ok {
        return Err("operation is not allowed for this tool");
    }
    if !matches!(call.tool, ToolName::AggregateProjects) && query.limit.unwrap_or(8) > 8 {
        return Err("record retrieval is limited to 8 projects");
    }
    Ok(())
}

fn contains_sql_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, nested)| {
            key.to_ascii_lowercase().contains("sql") || contains_sql_key(nested)
        }),
        Value::Array(values) => values.iter().any(contains_sql_key),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_whitelist_and_rejects_sql_or_internal_ids() {
        let valid = r#"{"answerMode":"analytical","calls":[{"tool":"get_risk_evidence","query":{"entity":"project","operation":"list","filters":[{"field":"project_manager","operator":"person_name_equals","value":"WU Wei"}],"limit":8},"entityField":null,"value":null}],"confidence":0.94,"clarificationQuestion":null}"#;
        assert_eq!(parse(valid).unwrap().calls.len(), 1);
        assert!(parse(r#"{"answerMode":"factual","calls":[],"confidence":0.9,"clarificationQuestion":null,"sql":"select *"}"#).is_err());
        let ids = r#"{"answerMode":"factual","calls":[{"tool":"aggregate_projects","query":{"entity":"project","operation":"count","filters":[{"field":"project_id","operator":"in","values":[1]}]},"entityField":null,"value":null}],"confidence":0.9,"clarificationQuestion":null}"#;
        assert!(parse(ids).is_err());

        let owner = r#"{"answerMode":"factual","calls":[{"tool":"aggregate_projects","query":{"entity":"owner","operation":"distinct","target":"project_manager","filters":[{"field":"mcr_id","operator":"equals","value":"BM-00108495"}],"limit":8},"entityField":null,"value":null}],"confidence":0.98,"clarificationQuestion":null}"#;
        let owner_plan = parse(owner).unwrap();
        assert_eq!(owner_plan.calls[0].tool, ToolName::AggregateProjects);

        let malformed = r#"{"answerMode":"factual","calls":[{"tool":"search_projects","query":null,"entityField":"secret","value":"BM"}],"confidence":0.9,"clarificationQuestion":null}"#;
        assert!(parse(malformed).is_err());

        let existence = r#"{"answerMode":"factual","calls":[{"tool":"search_projects","query":null,"entityField":"customer","value":"ACME"}],"confidence":0.95,"clarificationQuestion":null}"#;
        assert_eq!(
            parse(existence).unwrap().calls[0].tool,
            ToolName::SearchProjects
        );

        let mixed_arguments = r#"{"answerMode":"factual","calls":[{"tool":"search_projects","query":{"entity":"customer","operation":"list","filters":[]},"entityField":"customer","value":"ACME"}],"confidence":0.95,"clarificationQuestion":null}"#;
        assert_eq!(
            parse(mixed_arguments).unwrap_err(),
            "search_projects requires query:null and accepts only entityField plus value"
        );
    }

    #[test]
    fn enforces_bounded_multistep_plans_and_tool_specific_operations() {
        let call = ToolCall {
            tool: ToolName::AggregateProjects,
            query: Some(StructuredQuery {
                entity: crate::qa::structured::Entity::Project,
                operation: Operation::Count,
                target: None,
                filters: vec![],
                sort: None,
                limit: None,
            }),
            entity_field: None,
            value: None,
        };
        let too_many = QueryPlan {
            answer_mode: AnswerMode::Analytical,
            calls: vec![call; 5],
            confidence: 0.9,
            clarification_question: None,
        };
        assert_eq!(validate(&too_many).unwrap_err(), "too many retrieval calls");
        let unbounded_details = ToolCall {
            tool: ToolName::GetProjectDetails,
            query: Some(StructuredQuery {
                entity: crate::qa::structured::Entity::Project,
                operation: Operation::List,
                target: None,
                filters: vec![],
                sort: None,
                limit: Some(8),
            }),
            entity_field: None,
            value: None,
        };
        assert!(validate_call(&unbounded_details).is_err());
    }

    #[test]
    fn portfolio_scope_requires_an_unfiltered_analytical_retrieval() {
        let plan = QueryPlan {
            answer_mode: AnswerMode::Analytical,
            calls: vec![ToolCall {
                tool: ToolName::GetRiskEvidence,
                query: Some(StructuredQuery {
                    entity: crate::qa::structured::Entity::Project,
                    operation: Operation::Top,
                    target: None,
                    filters: vec![],
                    sort: Some(crate::qa::structured::Sort {
                        field: Field::RiskScore,
                        direction: crate::qa::structured::Direction::Desc,
                    }),
                    limit: Some(8),
                }),
                entity_field: None,
                value: None,
            }],
            confidence: 0.9,
            clarification_question: None,
        };
        assert!(is_portfolio_scope(&plan));
    }

    #[test]
    fn retrieval_review_allows_one_bounded_second_round_and_rejects_sql() {
        assert_eq!(
            parse_review(r#"{"decision":"answer","calls":[],"clarificationQuestion":null}"#)
                .unwrap()
                .decision,
            ReviewDecision::Answer
        );
        let more = r#"{"decision":"retrieve_more","calls":[{"tool":"search_projects","query":null,"entityField":"project_name","value":"Alpha"}],"clarificationQuestion":null}"#;
        assert_eq!(parse_review(more).unwrap().calls.len(), 1);
        assert!(parse_review(r#"{"decision":"retrieve_more","calls":[],"clarificationQuestion":null,"sql":"select *"}"#).is_err());
    }
}
