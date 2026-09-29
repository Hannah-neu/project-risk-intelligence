use crate::{
    ai_query_plan::{QueryPlan, ToolCall, ToolName},
    enterprise_context,
    enterprise_llm::EnterpriseQuestion,
    qa::{
        self,
        structured::{Field, Operator, StructuredQuery},
    },
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};

const MAX_CONTEXT_BYTES: usize = 16_000;

pub struct RetrievalBundle {
    pub value: Value,
    pub sufficient: bool,
}

pub fn merge(
    mut first: RetrievalBundle,
    second: RetrievalBundle,
) -> Result<RetrievalBundle, String> {
    let additional = second
        .value
        .get("toolResults")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let result_count = {
        let target = first
            .value
            .get_mut("toolResults")
            .and_then(Value::as_array_mut)
            .ok_or("Invalid retrieval bundle.")?;
        target.extend(additional);
        target.len()
    };
    first.sufficient &= second.sufficient;
    first.value["retrievalAssessment"]["status"] = json!(if first.sufficient {
        "sufficient"
    } else {
        "insufficient"
    });
    first.value["retrievalAssessment"]["includedRecords"] = json!(result_count);
    if serde_json::to_vec(&first.value)
        .map_err(|_| "Could not serialize AI retrieval context.")?
        .len()
        > MAX_CONTEXT_BYTES
    {
        return Err("The controlled AI context is too large; narrow the question.".into());
    }
    Ok(first)
}

pub fn execute(
    connection: &Connection,
    question: &str,
    _language: &str,
    plan: &QueryPlan,
) -> Result<RetrievalBundle, String> {
    let mut results = Vec::new();
    let mut sufficient = true;
    for (index, call) in plan.calls.iter().enumerate() {
        let result = execute_call(connection, question, call)?;
        #[cfg(debug_assertions)]
        eprintln!(
            "AI tool_step={} tool={:?} validation=accepted result_records={}",
            index + 1,
            call.tool,
            result_record_count(&result)
        );
        sufficient &= result
            .get("retrievalAssessment")
            .and_then(|value| value.get("status"))
            .and_then(Value::as_str)
            .is_none_or(|status| status != "insufficient");
        results.push(json!({"step": index + 1, "tool": call.tool, "result": result}));
    }
    let value = json!({
        "scope": "controlledToolRetrieval",
        "retrievalAssessment": {"status": if sufficient { "sufficient" } else { "insufficient" }, "includedRecords": results.len(), "reason": if sufficient { Value::Null } else { json!("One or more controlled retrieval steps found no usable evidence.") }},
        "toolResults": results
    });
    if serde_json::to_vec(&value)
        .map_err(|_| "Could not serialize AI retrieval context.")?
        .len()
        > MAX_CONTEXT_BYTES
    {
        return Err("The controlled AI context is too large; narrow the question.".into());
    }
    Ok(RetrievalBundle { value, sufficient })
}

fn result_record_count(result: &Value) -> usize {
    result
        .get("matches")
        .and_then(Value::as_array)
        .map(Vec::len)
        .or_else(|| {
            result
                .pointer("/answer/projects")
                .and_then(Value::as_array)
                .map(Vec::len)
        })
        .or_else(|| {
            result
                .pointer("/answer/groups")
                .and_then(Value::as_array)
                .map(Vec::len)
        })
        .or_else(|| {
            result
                .pointer("/answer/items")
                .and_then(Value::as_array)
                .map(Vec::len)
        })
        .unwrap_or_else(|| usize::from(result.get("answer").is_some()))
}

fn execute_call(connection: &Connection, question: &str, call: &ToolCall) -> Result<Value, String> {
    if call.tool == ToolName::GetBusinessSchema {
        return Ok(business_schema());
    }
    if call.tool == ToolName::SearchProjects {
        return search_projects(
            connection,
            call.entity_field,
            call.value.as_deref().unwrap_or_default(),
        );
    }
    if call.tool == ToolName::ResolveEntity {
        return resolve_entity(
            connection,
            call.entity_field.ok_or("Missing entity field.")?,
            call.value.as_deref().unwrap_or_default(),
        );
    }
    let mut query = call.query.clone().ok_or("Missing structured query.")?;
    resolve_and_validate(connection, &mut query)?;
    let answer = qa::structured::answer(connection, &query)?;
    let answer_value = serde_json::to_value(&answer)
        .map_err(|_| "Could not serialize controlled query result.")?;
    if call.tool == ToolName::AggregateProjects {
        return Ok(
            json!({"query": query, "answer": answer_value, "retrievalAssessment": {"status":"sufficient"}}),
        );
    }
    query.limit = Some(query.limit.unwrap_or(8).min(8));
    let request = EnterpriseQuestion {
        question: question.to_string(),
        language: "en".into(),
        structured_query: Some(query.clone()),
        last_project_ids: vec![],
        dataset_version: None,
        requires_current_dataset: false,
        direct_conversation: false,
        recent_messages: vec![],
    };
    let context = enterprise_context::build(connection, &request)?;
    Ok(
        json!({"query": query, "answer": answer_value, "evidence": context.value, "retrievalAssessment": context.assessment}),
    )
}

fn business_schema() -> Value {
    json!({
        "source": "applicationBusinessConfiguration",
        "entities": {
            "project": ["mcr_id", "project_name", "customer", "project_manager", "prj_status", "system", "product_category", "components"],
            "risk": ["risk_score", "risk_level", "risk_reasons", "primary_risk_driver"],
            "milestone": ["milestone_type", "planned_date", "actual_date", "delta_days", "status"]
        },
        "riskLevels": ["Low", "Medium", "High", "Critical"],
        "milestoneStatuses": ["Completed on time", "Completed late", "Overdue", "Not completed", "Not planned"],
        "grounding": "PRJ Status is imported. Risk Score and Risk Level are calculated by the official Risk Engine. Overdue means planned_date is before today and actual_date is null.",
        "retrievalAssessment": {"status": "sufficient"}
    })
}

fn search_projects(
    connection: &Connection,
    field: Option<Field>,
    value: &str,
) -> Result<Value, String> {
    let needle = value.trim();
    if needle.is_empty() || needle.chars().count() > 120 {
        return Err("Project search value is invalid.".into());
    }
    let predicate = match field {
        Some(Field::McrId) => "INSTR(LOWER(p.mcr_id),LOWER(?1))>0",
        Some(Field::ProjectName) => "INSTR(LOWER(p.project_name),LOWER(?1))>0",
        Some(Field::Customer) => "INSTR(LOWER(COALESCE(p.customer,'')),LOWER(?1))>0",
        Some(Field::Owner | Field::ProjectManager) => "INSTR(LOWER(COALESCE(p.project_manager,'')),LOWER(?1))>0",
        Some(Field::PrjStatus) => "INSTR(LOWER(COALESCE(p.project_status,'')),LOWER(?1))>0",
        Some(Field::System) => "INSTR(LOWER(COALESCE(p.system,'')),LOWER(?1))>0",
        Some(Field::ProductCategory) => "INSTR(LOWER(COALESCE(p.product_category,'')),LOWER(?1))>0",
        Some(Field::Component | Field::NewReleasedComponent) => "EXISTS (SELECT 1 FROM project_components c WHERE c.project_id=p.id AND INSTR(LOWER(c.component_name),LOWER(?1))>0)",
        Some(_) => return Err("This field is not searchable.".into()),
        None => "(INSTR(LOWER(p.mcr_id),LOWER(?1))>0 OR INSTR(LOWER(p.project_name),LOWER(?1))>0 OR INSTR(LOWER(COALESCE(p.customer,'')),LOWER(?1))>0 OR INSTR(LOWER(COALESCE(p.project_manager,'')),LOWER(?1))>0 OR INSTR(LOWER(COALESCE(p.system,'')),LOWER(?1))>0)",
    };
    let sql = format!("SELECT p.id,p.mcr_id,p.project_name,COALESCE(p.customer,''),COALESCE(p.project_manager,''),COALESCE(p.project_status,''),COALESCE(r.risk_level,'Low'),COALESCE(r.risk_score,0) FROM projects p LEFT JOIN project_risks r ON r.project_id=p.id WHERE {predicate} ORDER BY r.risk_score DESC,p.project_name COLLATE NOCASE LIMIT 8");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|_| "Could not search projects.".to_string())?;
    let rows = statement.query_map(params![needle], |row| Ok(json!({
        "id": row.get::<_, i64>(0)?, "mcrId": row.get::<_, String>(1)?, "projectName": row.get::<_, String>(2)?,
        "customer": row.get::<_, String>(3)?, "projectManager": row.get::<_, String>(4)?, "prjStatus": row.get::<_, String>(5)?,
        "riskLevel": row.get::<_, String>(6)?, "riskScore": row.get::<_, i64>(7)?
    }))).map_err(|_| "Could not search projects.".to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|_| "Could not search projects.".to_string())?;
    let status = if rows.is_empty() {
        "insufficient"
    } else {
        "sufficient"
    };
    Ok(json!({"searchValue": needle, "matches": rows, "retrievalAssessment": {"status": status}}))
}

fn resolve_and_validate(
    connection: &Connection,
    query: &mut StructuredQuery,
) -> Result<(), String> {
    if query.filters.len() > 12 || query.limit.is_some_and(|limit| limit == 0 || limit > 50) {
        return Err("The planned query exceeded safe limits.".into());
    }
    for filter in &mut query.filters {
        if filter.field == Field::ProjectId
            || filter.operator == Operator::In
            || filter.values.is_some()
        {
            return Err(
                "Internal project identifiers are not accepted from the AI planner.".into(),
            );
        }
        let Some(value) = filter
            .value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if let Some((table, column)) = entity_column(filter.field) {
            if matches!(
                filter.operator,
                Operator::Equals | Operator::PersonNameEquals | Operator::SurnameEquals
            ) {
                let candidates = candidates(connection, table, column)?;
                let matched = candidates
                    .iter()
                    .find(|candidate| match filter.operator {
                        Operator::PersonNameEquals => {
                            qa::structured::normalize_person_name(candidate)
                                == qa::structured::normalize_person_name(value)
                        }
                        Operator::SurnameEquals => {
                            qa::structured::normalize_person_name(candidate)
                                .split_whitespace()
                                .next()
                                == qa::structured::normalize_person_name(value)
                                    .split_whitespace()
                                    .next()
                        }
                        _ => candidate.trim().eq_ignore_ascii_case(value),
                    })
                    .ok_or_else(|| {
                        format!(
                            "The entity '{value}' does not exist in the current SQLite dataset."
                        )
                    })?;
                if filter.operator == Operator::Equals {
                    filter.value = Some(matched.clone());
                }
            }
        }
    }
    qa::structured::answer(connection, query).map(|_| ())
}

fn resolve_entity(connection: &Connection, field: Field, value: &str) -> Result<Value, String> {
    let (table, column) = entity_column(field).ok_or("This entity cannot be resolved.")?;
    let needle = value.trim().to_lowercase();
    if needle.is_empty() {
        return Err("Entity value is empty.".into());
    }
    let matches = candidates(connection, table, column)?
        .into_iter()
        .filter(|candidate| candidate.to_lowercase().contains(&needle))
        .take(5)
        .collect::<Vec<_>>();
    let status = if matches.is_empty() {
        "insufficient"
    } else if matches.len() == 1 {
        "sufficient"
    } else {
        "partial"
    };
    Ok(
        json!({"entityField": field, "requestedValue": value, "matches": matches, "retrievalAssessment": {"status": status}}),
    )
}

fn entity_column(field: Field) -> Option<(&'static str, &'static str)> {
    match field {
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
    }
}

fn candidates(connection: &Connection, table: &str, column: &str) -> Result<Vec<String>, String> {
    let sql = format!("SELECT DISTINCT {column} FROM {table} WHERE {column} IS NOT NULL AND TRIM({column})<>'' ORDER BY {column} COLLATE NOCASE LIMIT 500");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|_| "Could not resolve an entity against SQLite.")?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| "Could not resolve an entity against SQLite.")?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|_| "Could not resolve an entity against SQLite.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ai_query_plan::{AnswerMode, ToolCall},
        database::migrations,
        qa::structured::{Entity, Filter, Operation},
    };
    fn fixture() -> Connection {
        let mut db = Connection::open_in_memory().unwrap();
        migrations::run(&mut db).unwrap();
        db.execute_batch("INSERT INTO projects(id,mcr_id,project_name,customer,project_manager,source_file,source_row) VALUES(1,'BM-1','Alpha','ABC','WU Wei','x',1); INSERT INTO project_risks(project_id,risk_score,risk_level,risk_reasons) VALUES(1,90,'Critical','[\"Overdue SE3\"]');").unwrap();
        db
    }
    #[test]
    fn controlled_aggregate_is_parameterized_and_entity_checked() {
        let db = fixture();
        let query = StructuredQuery {
            entity: Entity::Project,
            operation: Operation::Count,
            target: None,
            filters: vec![Filter {
                field: Field::Customer,
                operator: Operator::Equals,
                value: Some("ABC".into()),
                values: None,
            }],
            sort: None,
            limit: None,
        };
        let plan = QueryPlan {
            answer_mode: AnswerMode::Factual,
            calls: vec![ToolCall {
                tool: ToolName::AggregateProjects,
                query: Some(query.clone()),
                entity_field: None,
                value: None,
            }],
            confidence: 0.9,
            clarification_question: None,
        };
        assert_eq!(
            execute(&db, "Customer ABC count", "en", &plan)
                .unwrap()
                .value["toolResults"][0]["result"]["answer"]["value"],
            1
        );
        let mut unknown = query;
        unknown.filters[0].value = Some("Missing".into());
        let plan = QueryPlan {
            answer_mode: AnswerMode::Factual,
            calls: vec![ToolCall {
                tool: ToolName::AggregateProjects,
                query: Some(unknown),
                entity_field: None,
                value: None,
            }],
            confidence: 0.9,
            clarification_question: None,
        };
        assert!(execute(&db, "missing", "en", &plan).is_err());
    }

    #[test]
    fn multistep_risk_retrieval_is_bounded_and_entity_resolution_exposes_ambiguity() {
        let db = fixture();
        db.execute_batch("INSERT INTO projects(id,mcr_id,project_name,customer,project_manager,source_file,source_row) VALUES(2,'BM-2','Beta','ABD','WU Wei','x',2); INSERT INTO project_risks(project_id,risk_score,risk_level,risk_reasons) VALUES(2,60,'High','[\"Late SE2\"]');").unwrap();
        let risk_call = |customer: &str| ToolCall {
            tool: ToolName::GetRiskEvidence,
            query: Some(StructuredQuery {
                entity: Entity::Project,
                operation: Operation::List,
                target: None,
                filters: vec![Filter {
                    field: Field::Customer,
                    operator: Operator::Equals,
                    value: Some(customer.into()),
                    values: None,
                }],
                sort: None,
                limit: Some(8),
            }),
            entity_field: None,
            value: None,
        };
        let plan = QueryPlan {
            answer_mode: AnswerMode::Analytical,
            calls: vec![risk_call("ABC"), risk_call("ABD")],
            confidence: 0.9,
            clarification_question: None,
        };
        let bundle = execute(&db, "Compare ABC and ABD risks", "en", &plan).unwrap();
        assert!(bundle.sufficient);
        assert!(serde_json::to_vec(&bundle.value).unwrap().len() < MAX_CONTEXT_BYTES);

        let resolve = ToolCall {
            tool: ToolName::ResolveEntity,
            query: None,
            entity_field: Some(Field::Customer),
            value: Some("AB".into()),
        };
        let plan = QueryPlan {
            answer_mode: AnswerMode::Analytical,
            calls: vec![resolve],
            confidence: 0.9,
            clarification_question: None,
        };
        let bundle = execute(&db, "customer AB", "en", &plan).unwrap();
        assert_eq!(
            bundle.value["toolResults"][0]["result"]["retrievalAssessment"]["status"],
            "partial"
        );
    }

    #[test]
    fn project_search_and_business_schema_are_bounded_read_only_tools() {
        let db = fixture();
        let search = ToolCall {
            tool: ToolName::SearchProjects,
            query: None,
            entity_field: Some(Field::McrId),
            value: Some("BM-1".into()),
        };
        let schema = ToolCall {
            tool: ToolName::GetBusinessSchema,
            query: None,
            entity_field: None,
            value: None,
        };
        let plan = QueryPlan {
            answer_mode: AnswerMode::Analytical,
            calls: vec![search, schema],
            confidence: 0.95,
            clarification_question: None,
        };
        let bundle = execute(&db, "Explain BM-1", "en", &plan).unwrap();
        assert_eq!(
            bundle.value["toolResults"][0]["result"]["matches"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            bundle.value["toolResults"][1]["result"]["source"],
            "applicationBusinessConfiguration"
        );
    }
}
