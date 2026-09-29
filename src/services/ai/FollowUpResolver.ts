import type { AIAnswer } from "./types";
import type { QueryFilter, StructuredQuery } from "./StructuredQuery";
import { findStage, hasTerm } from "./terminology";

/** Only structured, bounded query state is retained; chat text is not used as a query plan. */
export interface ConversationQueryState {
  query: StructuredQuery;
  lastIntent: AIAnswer["answerType"];
  lastOperation: StructuredQuery["operation"];
  resultProjectIds: number[];
  resultValues: string[];
  resultTotal?: number;
  datasetVersion: string;
}

export type FollowUpResolution =
  | { kind: "local"; query: StructuredQuery }
  | { kind: "analytical"; query: StructuredQuery }
  | { kind: "clarify" };

export function isFollowUp(question: string): boolean {
  const q = question.trim().replace(/[?？.。!！]+$/, "").trim();
  if (/^哪些项目(?:是|有|的|\s)/u.test(q) || /^which projects?\s+(?:are|have|with|for|managed|belong)/iu.test(q)) return false;
  return /^(?:是哪几个|哪几个|分别是哪些|是哪些|有哪些|哪些是|哪些逾期|哪些超期|哪些项目$|项目经理(?:分别)?是谁|一共有几个|总共有几个|有几个|多少个|为什么|为何|风险怎么样|有什么共同风险|哪个更值得关注|其中|这些|那几个|它们|他们|她们|those|these|them|why\b|how risky are they|how many (?:are )?there|which (?:ones|are|of (?:those|them))|which projects?$|what are they|list them|what about those|and which|what risks? do they share|which (?:one|ones) (?:need|deserve))/iu.test(q);
}

function additionalFilters(question: string): QueryFilter[] | null {
  const q = question.trim();
  const filters: QueryFilter[] = [];
  const risk = q.match(/\b(?:Critical|High|Medium|Low)\b(?:\s+risk)?|严重风险|高风险|中风险|低风险/iu)?.[0];
  if (risk) {
    const value = /critical|严重/iu.test(risk) ? "Critical" : /high|高/iu.test(risk) ? "High" : /medium|中/iu.test(risk) ? "Medium" : "Low";
    filters.push({ field: "risk_level", operator: "equals", value });
  }
  const status = q.match(/\b(?:WORK|HOLD|END|LOST|PLAN|PREP|RESV|WISH)\b/iu)?.[0];
  if (status) filters.push({ field: "prj_status", operator: "equals", value: status.toUpperCase() });
  const stage = findStage(q);
  if (hasTerm(q, "overdue")) filters.push({ field: "milestone", operator: "overdue", value: stage });
  else if (stage) return null; // A stage alone does not specify a safe milestone predicate.
  const allowed = q.replace(/\b(?:Critical|High|Medium|Low)(?:\s+risk)?|严重风险|高风险|中风险|低风险|\b(?:WORK|HOLD|END|LOST|PLAN|PREP|RESV|WISH)\b|\b(?:SE2|SE3|SE4|QGC0|QGC4)\b|overdue|逾期|超期|已过期|项目经理|负责人|工程师|分别|是谁|project managers?|owners?|who/giu, "")
    .replace(/^(?:其中|这些|那几个|它们|哪些|哪几个|是|的|项目|which|of|those|them|they|are|and|list|projects?|status|状态|the|in|on|have|with|ones|what|about|how many|多少|几个|有|一共|总共|是哪些|有哪些|\s)+/giu, "")
    .replace(/[?？.。!！\s]+$/g, "");
  return allowed ? null : filters;
}

export function resolveFollowUp(question: string, state: ConversationQueryState | null): FollowUpResolution | null {
  if (!isFollowUp(question)) return null;
  if (!state) return { kind: "clarify" };
  const q = question.trim().replace(/[?？.。!！]+$/, "");
  const isWhy = /^(?:为什么|为何|风险怎么样|有什么共同风险|哪个更值得关注|why\b|how risky are they|what risks? do they share|which (?:one|ones) (?:need|deserve))/iu.test(q);
  const isCount = /^(?:一共|总共)?(?:有几个|多少个)|^how many/iu.test(q);
  const isList = /^(?:是哪几个|哪几个|分别是哪些|是哪些|有哪些|哪些是|哪些逾期|哪些超期|哪些项目|项目经理|其中|这些|那几个|它们|他们|她们|those|these|them|which|what are they|list them|and which|what about those)/iu.test(q);
  if (!isWhy && !isCount && !isList) return { kind: "clarify" };
  const extra = isWhy ? [] : additionalFilters(q);
  if (extra === null) return { kind: "clarify" };
  // A complete prior list is an exact set. A truncated list must retain its original
  // filters instead, or the next answer would silently omit unshown projects.
  const exactIds = state.lastIntent === "project_list" && state.resultProjectIds.length > 0 && state.resultProjectIds.length === state.resultTotal;
  const ids: QueryFilter[] = exactIds ? [{ field: "project_id", operator: "in", values: state.resultProjectIds }] : [];
  const filters = [...state.query.filters.filter((filter) => filter.field !== "project_id"), ...ids, ...extra];
  if (filters.length > 12) return { kind: "clarify" };
  if (isWhy) {
    if (state.query.entity !== "project" && !filters.length) return { kind: "clarify" };
    return { kind: "analytical", query: { entity: "project", operation: "list", filters, limit: 8 } };
  }
  if (/项目经理|负责人|工程师|project managers?|owners?/iu.test(q) && /哪些|是谁|who|which/iu.test(q)) {
    return { kind: "local", query: { entity: "project", operation: "distinct", target: "project_manager", filters, limit: 50 } };
  }
  if (isCount) return { kind: "local", query: { entity: "project", operation: "count", filters } };
  return { kind: "local", query: { entity: "project", operation: "list", filters, limit: 50 } };
}

export function stateFromAnswer(query: StructuredQuery, answer: AIAnswer): ConversationQueryState | null {
  const datasetVersion = answer.metadata?.datasetVersion;
  if (answer.provider !== "local" || answer.answerType === "unsupported" || typeof datasetVersion !== "string") return null;
  return {
    query,
    lastIntent: answer.answerType,
    lastOperation: query.operation,
    resultProjectIds: answer.answerType === "project_list" ? answer.projects.map((project) => project.id) : [],
    resultValues: answer.answerType === "text" ? answer.items : [],
    resultTotal: answer.answerType === "project_list" ? answer.total : undefined,
    datasetVersion
  };
}
