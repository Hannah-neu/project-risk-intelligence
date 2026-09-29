const LATIN_NAME = "[A-Za-z][A-Za-z'-]*(?:\\s+[A-Za-z][A-Za-z'-]*){1,3}";
const EXPLICIT_NAME = "[A-Za-z][A-Za-z'-]*(?:\\s+[A-Za-z][A-Za-z'-]*){0,3}";
const STOP_WORDS = new Set(["and", "or", "with", "where", "who", "which", "project", "projects", "critical", "high", "risk", "risky", "overdue", "status", "own", "owns", "does", "have", "has"]);
const INVALID_FIRST_WORDS = new Set(["contains", "contain", "equals", "equal", "is", "null", "empty", "not", "customer", "status", "project", "projects", "risk", "critical", "high", "medium", "low", "system", "product", "category", "manager", "owner"]);

/** Comparison only: never replaces the name displayed or stored in the workbook/database. */
export function normalizePersonName(value: string): string {
  return value.split(/[（(]/, 1)[0].trim().replace(/\s+/g, " ").toUpperCase();
}

function cleanCandidate(value: string): string | null {
  const words = value.trim().split(/\s+/);
  while (words.length && STOP_WORDS.has(words[words.length - 1].toLowerCase())) words.pop();
  if (words.length && INVALID_FIRST_WORDS.has(words[0].toLowerCase())) return null;
  const candidate = words.join(" ").trim();
  return candidate && normalizePersonName(candidate) ? candidate : null;
}

/** Extract a full manager name only from a supported person/project relationship. */
export function extractPersonName(question: string): string | null {
  const q = question.trim();
  const patterns = [
    new RegExp(`(?:managed by|owned by|owner\\b|project manager\\b|项目经理|负责人)\\s*[:：]?\\s*(${EXPLICIT_NAME})`, "iu"),
    new RegExp(`^(?:目前|现在|当前)?\\s*(${LATIN_NAME})\\s*(?=负责|管理|有(?:多少|几个|哪些|什么)|的?\\s*项目)`, "iu"),
    new RegExp(`\\b(?:how many|which|show|list)\\s+projects?\\s+(?:does|do)\\s+(${LATIN_NAME})\\s+(?:own|manage|have)\\b`, "iu"),
    /^([\p{Script=Han}]{2,4})负责/u
  ];
  for (const pattern of patterns) {
    const value = q.match(pattern)?.[1];
    if (value) return cleanCandidate(value);
  }
  return null;
}

/** Used to refuse an unfiltered COUNT/LIST if a likely person mention was not resolved. */
export function hasUnresolvedPersonMention(question: string, filters: readonly { field: string; operator: string }[]): boolean {
  const q = question.trim();
  const person = extractPersonName(q);
  if (person) return !filters.some((filter) => filter.field === "project_manager" && filter.operator === "person_name_equals");
  const implicit = q.match(new RegExp(`^(?:目前|现在|当前)?\\s*(${LATIN_NAME})\\s*(?:有|负责|管理|的?\\s*项目)`, "iu"))?.[1]
    ?? q.match(new RegExp(`\\b(?:does|do)\\s+(${LATIN_NAME})\\s+(?:own|manage|have)\\b`, "iu"))?.[1];
  const implicitName = !!implicit && !!cleanCandidate(implicit);
  return implicitName && !filters.some((filter) => filter.field === "project_manager");
}
