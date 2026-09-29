import { invoke } from "@tauri-apps/api/core";
import type { FilterOperator, QueryEntity, QueryField, QueryOperation, StructuredQuery } from "./StructuredQuery";

export type SemanticIntent = "factual_query" | "project_filter" | "aggregation" | "analytical_question" | "general_conversation" | "unsupported";
export interface SemanticPlan { intent: SemanticIntent; query: StructuredQuery | null; confidence: number }

const entities = new Set<QueryEntity>(["project", "customer", "owner", "prj_status", "risk_level", "milestone", "component", "new_released_component", "system", "product_category"]);
const operations = new Set<QueryOperation>(["count", "count_distinct", "list", "distinct", "group_by", "max", "min", "top", "sort"]);
const fields = new Set<QueryField>(["project", "customer", "owner", "prj_status", "risk_level", "milestone", "component", "new_released_component", "system", "product_category", "project_manager", "mcr_id", "project_name", "risk_score", "planned_date", "actual_date"]);
const operators = new Set<FilterOperator>(["equals", "contains", "starts_with", "person_name_equals", "surname_equals", "greater_than", "less_than", "is_null", "is_not_null", "overdue", "completed_late"]);
const intents = new Set<SemanticIntent>(["factual_query", "project_filter", "aggregation", "analytical_question", "general_conversation", "unsupported"]);

export async function interpretSemanticQuestion(question: string): Promise<SemanticPlan> {
  const value = await invoke<unknown>("interpret_enterprise_question", { question });
  const plan = validateSemanticPlan(value);
  if (!plan) throw new Error("The Enterprise LLM returned an unsupported semantic query plan.");
  return plan;
}

export function validateSemanticPlan(value: unknown): SemanticPlan | null {
  if (!record(value) || hasSqlKey(value) || !onlyKeys(value, ["intent", "query", "confidence"])) return null;
  if (!intents.has(value.intent as SemanticIntent) || typeof value.confidence !== "number" || value.confidence < 0.55 || value.confidence > 1) return null;
  if (value.query === null) return ["general_conversation", "unsupported"].includes(value.intent as string) ? value as unknown as SemanticPlan : null;
  if (["general_conversation", "unsupported"].includes(value.intent as string) || !validQuery(value.query)) return null;
  return value as unknown as SemanticPlan;
}

function validQuery(value: unknown): value is StructuredQuery {
  if (!record(value) || !onlyKeys(value, ["entity", "operation", "target", "filters", "sort", "limit"])) return false;
  if (!entities.has(value.entity as QueryEntity) || !operations.has(value.operation as QueryOperation) || !Array.isArray(value.filters) || value.filters.length === 0 || value.filters.length > 12) return false;
  if (value.target !== undefined && !fields.has(value.target as QueryField)) return false;
  if (value.limit !== undefined && (!Number.isInteger(value.limit) || (value.limit as number) < 1 || (value.limit as number) > 50)) return false;
  if (value.sort !== undefined && (!record(value.sort) || !onlyKeys(value.sort, ["field", "direction"]) || !fields.has(value.sort.field as QueryField) || !["asc", "desc"].includes(value.sort.direction as string))) return false;
  return value.filters.every((filter) => record(filter) && onlyKeys(filter, ["field", "operator", "value"]) && fields.has(filter.field as QueryField) && filter.field !== "project_id" && operators.has(filter.operator as FilterOperator) && (filter.value === undefined || typeof filter.value === "string"));
}

function record(value: unknown): value is Record<string, unknown> { return typeof value === "object" && value !== null && !Array.isArray(value); }
function onlyKeys(value: Record<string, unknown>, allowed: string[]): boolean { return Object.keys(value).every((key) => allowed.includes(key)); }
function hasSqlKey(value: unknown): boolean {
  if (Array.isArray(value)) return value.some(hasSqlKey);
  return record(value) && Object.entries(value).some(([key, child]) => key.toLowerCase().includes("sql") || hasSqlKey(child));
}
