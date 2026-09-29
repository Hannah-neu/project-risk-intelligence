import type { NormalizedQuestion } from "./QuestionNormalizer";
import { findStage, hasTerm } from "./terminology";
import { extractPersonName, hasUnresolvedPersonMention } from "./personName";

export type QueryEntity = "project" | "customer" | "owner" | "prj_status" | "risk_level" | "milestone" | "component" | "new_released_component" | "system" | "product_category";
export type QueryField = QueryEntity | "project_id" | "project_manager" | "mcr_id" | "project_name" | "risk_score" | "planned_date" | "actual_date";
export type QueryOperation = "count" | "count_distinct" | "list" | "distinct" | "group_by" | "max" | "min" | "top" | "sort";
export type FilterOperator = "equals" | "contains" | "starts_with" | "person_name_equals" | "surname_equals" | "greater_than" | "less_than" | "is_null" | "is_not_null" | "overdue" | "completed_late" | "in";
export interface QueryFilter { field: QueryField; operator: FilterOperator; value?: string; values?: number[] }
export interface StructuredQuery {
  entity: QueryEntity;
  operation: QueryOperation;
  target?: QueryField;
  filters: QueryFilter[];
  sort?: { field: QueryField; direction: "asc" | "desc" };
  limit?: number;
}

const dimensions: { entity: QueryEntity; pattern: RegExp }[] = [
  { entity: "owner", pattern: /\b(?:owners?|project managers?|engineers?)\b|项目经理|负责人|工程师/iu },
  { entity: "customer", pattern: /\bcustomers?\b|客户/iu },
  { entity: "prj_status", pattern: /\b(?:prj\s*)?status(?:es)?\b|项目状态|状态/iu },
  { entity: "risk_level", pattern: /\brisk levels?\b|风险等级/iu },
  { entity: "new_released_component", pattern: /\bnew(?:ly)? released components?\b|新发布组件/iu },
  { entity: "component", pattern: /\bcomponents?\b|零部件|组件/iu },
  { entity: "system", pattern: /\bsystems?\b|系统/iu },
  { entity: "product_category", pattern: /\bproduct categor(?:y|ies)\b|产品类别/iu },
  { entity: "milestone", pattern: /\bmilestones?\b|里程碑|节点/iu }
];
const cleanValue = (value: string) => value.trim().replace(/^[“”"'‘’]+|[“”"'‘’?.。！!，,]+$/g, "").trim();
const capture = (q: string, pattern: RegExp) => cleanValue(q.match(pattern)?.slice(1).find(Boolean) ?? "");

function filtersFromQuestion(q: string): QueryFilter[] {
  const filters: QueryFilter[] = [];
  const explicitFields: { field: QueryField; label: string }[] = [
    { field: "customer", label: "(?:customer|客户)" }, { field: "owner", label: "(?:owner|project manager|项目经理|负责人)" },
    { field: "system", label: "(?:system|系统)" }, { field: "product_category", label: "(?:product category|产品类别)" },
    { field: "prj_status", label: "(?:PRJ status|项目状态)" }
  ];
  const risk = q.match(/\bcritical\b|\b(?:high|medium|low)[ -]risk\b|严重风险|高风险|中风险|低风险/i)?.[0];
  if (risk) {
    const value = /critical|严重/.test(risk.toLowerCase()) ? "Critical" : /high|高/.test(risk.toLowerCase()) ? "High" : /medium|中/.test(risk.toLowerCase()) ? "Medium" : "Low";
    filters.push({ field: "risk_level", operator: "equals", value });
  }
  const customer = capture(q, /\bcustomer\s+([\p{L}\p{N}_-]+)\b|客户\s*([\p{L}\p{N}_-]+)/iu) || capture(q, /客户\s*([\p{L}\p{N}_-]+)/u);
  if (customer && !/^(?:x|which|with|has|have|count|projects?|most|contains?|equals?|is|null|empty|not)$/i.test(customer)) filters.push({ field: "customer", operator: "equals", value: customer });
  const surname = capture(q, /姓\s*([\p{L}]+)|\bsurname\s+([\p{L}]+)\b/iu);
  if (surname) filters.push({ field: "project_manager", operator: "surname_equals", value: surname });
  else {
    const person = extractPersonName(q);
    if (person && !/^(?:contains?|equals?|is|null|empty|not)\b/i.test(person)) filters.push({ field: "project_manager", operator: "person_name_equals", value: person });
  }
  const status = capture(q, /\b([A-Z]{3,10})\s*状态/u)
    || capture(q, /(?:\bPRJ\s+Status\b|\bstatus\b)\s*[:：]?\s*([A-Z]{3,10})\b/u)
    || (/\bHOLD\b/i.test(q) ? "HOLD" : "");
  if (status && !["PRJ", "AND", "THE", "FOR", "WITH"].includes(status)) filters.push({ field: "prj_status", operator: "equals", value: status });
  const stage = findStage(q);
  const overdue = hasTerm(q, "overdue");
  const completedLate = /completed late|late completion|已完成.*(?:延迟|逾期)|完成延迟/iu.test(q);
  if (overdue) filters.push({ field: "milestone", operator: "overdue", value: stage });
  else if (completedLate) filters.push({ field: "milestone", operator: "completed_late", value: stage });
  else if (stage) filters.push({ field: "milestone", operator: "equals", value: stage });
  const component = (/\b(?:customer|owner|project manager|system|product category|prj status)\s+contains?\b/iu.test(q) ? "" : capture(q, /(?:\b(?:containing|contains?)\b|包含|含有)\s*([\p{L}\p{N}_-]+)(?:\s+components?)?/iu))
    || capture(q, /(?:new(?:ly)? released|新发布)\s*([\p{L}\p{N}_-]+)\s*(?:components?|组件)/iu);
  if (component && !/^(?:a|the|new|released|component|components)$/i.test(component)) filters.push({ field: /new(?:ly)? released|新发布/iu.test(q) ? "new_released_component" : "component", operator: "contains", value: component });
  else if (/new(?:ly)? released components?|新发布组件/iu.test(q) && /\bprojects?\b|项目/u.test(q)) filters.push({ field: "new_released_component", operator: "is_not_null" });
  const system = capture(q, /\bsystem\s+([\p{L}\p{N}_-]+)\b|系统\s*([\p{L}\p{N}_-]+)/iu);
  if (system && !/^(?:x|which|with|has|have|projects?|contains?|equals?|is|null|empty|not)$/i.test(system)) filters.push({ field: "system", operator: "equals", value: system });
  const category = capture(q, /\bproduct category\s+([\p{L}\p{N}_-]+)\b|产品类别\s*([\p{L}\p{N}_-]+)/iu);
  if (category) filters.push({ field: "product_category", operator: "equals", value: category });
  const mcr = q.match(/\b(?:BM|MCR)-[\w-]+\b/i)?.[0];
  if (mcr) filters.push({ field: "mcr_id", operator: "equals", value: mcr });
  for (const { field, label } of explicitFields) {
    const nullExpression = new RegExp(`${label}\\s*(?:is\\s*)?(not null|not empty|is null|null|empty|为空|不为空)`, "iu");
    const nullMatch = q.match(nullExpression);
    if (nullMatch) { filters.push({ field, operator: /not|不/.test(nullMatch[1]) ? "is_not_null" : "is_null" }); continue; }
    const match = q.match(new RegExp(`${label}\\s*(contains?|包含|equals?|=)\\s*([\\p{L}\\p{N}_-]+)`, "iu"));
    if (match) { filters.push({ field, operator: /contain|包含/i.test(match[1]) ? "contains" : "equals", value: match[2] }); }
  }
  const score = q.match(/(?:risk score|风险分数)\s*(?:>|greater than|above|超过|高于)\s*(\d{1,3})/iu);
  if (score) filters.push({ field: "risk_score", operator: "greater_than", value: score[1] });
  const scoreBelow = q.match(/(?:risk score|风险分数)\s*(?:<|less than|below|低于)\s*(\d{1,3})/iu);
  if (scoreBelow) filters.push({ field: "risk_score", operator: "less_than", value: scoreBelow[1] });
  for (const [field, pattern] of [["planned_date", /(?:plan(?:ned)? date|计划日期)\s*(?:is\s*)?(?:empty|null|missing|为空)/iu], ["actual_date", /(?:actual date|实际日期)\s*(?:is\s*)?(?:empty|null|missing|为空)/iu]] as const) {
    if (pattern.test(q)) filters.push({ field, operator: "is_null" });
  }
  for (const field of ["planned_date", "actual_date"] as const) {
    const label = field === "planned_date" ? "(?:planned? date|计划日期)" : "(?:actual date|实际日期)";
    if (new RegExp(`${label}\\s*(?:is\\s*)?(?:not null|not empty|非空|不为空)`, "iu").test(q)) filters.push({ field, operator: "is_not_null" });
    const date = q.match(new RegExp(`${label}\\s*(?:>|<|after|before|晚于|早于)\\s*(\\d{4}-\\d{2}-\\d{2})`, "iu"));
    if (date) filters.push({ field, operator: />|after|晚于/iu.test(date[0]) ? "greater_than" : "less_than", value: date[1] });
  }
  return filters;
}

export function parseStructuredQuery(normalized: NormalizedQuestion, forAnalysis = false): StructuredQuery | null {
  const q = normalized.original.trim().replace(/[?？.。!！]+$/, "");
  if (!q || q.length > 500) return null;
  // “Risk project” has no single configured threshold. Return the complete,
  // deterministic Risk Level distribution instead of guessing one category.
  if (/^(?:风险项目(?:的)?数量|风险项目有多少(?:个)?|有多少(?:个)?风险项目|risk projects? count|number of risk projects?|how many risk projects?(?: are there)?)$/iu.test(q)) {
    return { entity: "project", operation: "group_by", target: "risk_level", filters: [], limit: 4 };
  }
  const filters = filtersFromQuestion(q);
  if (hasUnresolvedPersonMention(q, filters)) return null;
  if (forAnalysis) return filters.length ? { entity: "project", operation: "list", filters, limit: 8 } : null;
  if (hasTerm(q, "analytical") || hasTerm(q, "negation") || /\b(?:or|between|before|after|since|this month|next year|unusual|attention)\b|或者|或|之间|今年|明年|本月|值得关注|需要关注/iu.test(q)) return null;
  const count = /^(?:how many|count\b|number of|total\b)|多少|几个|数量|总数/iu.test(q);
  const group = (/\b(?:by|per|group(?:ed)? by)\b|按.*(?:分组|统计)|哪个.*最多|哪些.*最多/iu.test(q) && /最多|most|top|group|按.*(?:分组|统计)/iu.test(q))
    || /\bwhich\s+(?:customers?|owners?|project managers?)\b.*\b(?:most|critical|high[ -]risk)\b/iu.test(q);
  const distinct = /\bdistinct\b|不同|去重|多少(?:个|位)?(?:不同|客户|工程师|负责人|项目经理)/iu.test(q);
  const max = /\b(?:maximum|max|highest)\b|最高|最大/iu.test(q);
  const min = /\b(?:minimum|min|lowest)\b|最低|最小/iu.test(q);
  const top = /\btop\s+\d*|前\s*\d+|最多的|最高的/iu.test(q);
  const sorted = /\b(?:sort|order(?:ed)? by)\b|排序/iu.test(q);
  const list = /\b(?:which|show|list|find|what|who)\b|哪些|列出|显示|查找|有什么|有谁/iu.test(q);
  if (!count && !group && !max && !min && !top && !list && !sorted) return null;

  const mentioned = dimensions.filter(({ pattern }) => pattern.test(q)).map(({ entity }) => entity);
  const asksProjects = /\bprojects?\b|项目(?!经理)/iu.test(q);
  if (asksProjects && !filters.length && !group && !max && !min && !top && !sorted && list && !/^(?:show|list)\s+projects?\b|^哪些项目[?？]?$|^列出项目/u.test(q)) return null;
  const entity: QueryEntity = asksProjects ? "project" : mentioned[0] ?? "project";
  if (!asksProjects && !mentioned.length && !filters.length && !/(?:risk score|风险分数)/iu.test(q)) return null;
  let target: QueryField | undefined;
  if (group) target = mentioned.find((item) => item !== "risk_level") ?? mentioned[0];
  const sort = /(?:risk score|风险分数)/iu.test(q) && (max || min || top || sorted) ? { field: "risk_score" as const, direction: min || /ascending|asc\b|升序|从低到高/iu.test(q) ? "asc" as const : "desc" as const } : undefined;
  const operation: QueryOperation = group && target ? "group_by" : count ? (distinct || ["customer", "owner", "prj_status", "risk_level", "system", "product_category"].includes(entity) ? "count_distinct" : "count") : max && !asksProjects ? "max" : min && !asksProjects ? "min" : top ? "top" : sorted ? "sort" : distinct || entity !== "project" ? "distinct" : "list";
  if (operation === "count" && entity === "project" && filters.length === 0 && !/^(?:how many projects(?: are there)?|count projects|total projects|有多少(?:个)?项目|项目总数(?:是多少)?|一共有多少(?:个)?项目)$/iu.test(q.trim())) return null;
  if (operation === "distinct" && entity === "project") return { entity, operation: "list", filters, sort, limit: 50 };
  if (operation === "max" || operation === "min") target = "risk_score";
  if ((operation === "top" || operation === "sort") && !sort && entity === "project") return null;
  const limitText = q.match(/\btop\s+(\d{1,2})\b|前\s*(\d{1,2})/iu);
  const limit = limitText ? Math.min(Number(limitText[1] ?? limitText[2]), 50) : operation === "group_by" && /哪个|\bwhich\s+(?:owner|project manager|customer|system)\s+has\s+the\s+most/iu.test(q) ? 1 : operation === "top" ? 10 : 50;
  return { entity, operation, target, filters, sort, limit };
}
