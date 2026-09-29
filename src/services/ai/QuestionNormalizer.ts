import { findStage, hasTerm } from "./terminology";

export type QuestionLanguage = "zh" | "en";

export interface NormalizedQuestion {
  original: string;
  canonical: string;
  language: QuestionLanguage;
}

function clean(value: string): string {
  return value.replace(/\s+/g, " ").trim().replace(/[?？.。!！]+$/, "").trim();
}

function detectLanguage(question: string): QuestionLanguage {
  // Chinese sentence structure wins for mixed-language business questions.
  if (/(哪些|多少|有无|有没有|的项目|项目有|属于|处于|负责|显示|查找|请列出|是什么|为什么|项目总数)/.test(question)) return "zh";
  const han = question.match(/[\p{Script=Han}]/gu)?.length ?? 0;
  const latin = question.match(/[A-Za-z]+/g)?.length ?? 0;
  return han > latin ? "zh" : "en";
}

function riskLevel(question: string): "Critical" | "High" | "Medium" | "Low" | undefined {
  if (hasTerm(question, "critical")) return "Critical";
  if (hasTerm(question, "high")) return "High";
  if (hasTerm(question, "medium")) return "Medium";
  if (hasTerm(question, "low")) return "Low";
  return undefined;
}

function entity(question: string, expressions: RegExp[]): string | undefined {
  for (const expression of expressions) {
    const value = question.match(expression)?.[1]?.trim();
    if (value) return value;
  }
  return undefined;
}

const CUSTOMER_PATTERNS = [
  /(?:客户|customer)\s*[:：]?\s*(.+?)\s*(?:有(?:哪些)?|的|旗下|相关).*?(?:项目|projects?)/i,
  /(?:显示|列出|查找)\s*(?:客户|customer)\s*[:：]?\s*(.+?)\s*的\s*(?:项目|projects?)/i
];
const MANAGER_PATTERNS = [
  /^(.+?)\s*(?:负责|owner)\s*(?:的|哪些)?\s*.*?(?:项目|projects?)/i,
  /(?:项目经理|负责人)\s*(?:是|为|[:：])?\s*(.+?)\s*的\s*(?:项目|projects?)/i
];

export function normalizeQuestion(question: string): NormalizedQuestion {
  const original = question.trim();
  const q = clean(original);
  const language = detectLanguage(q);
  if (!q) return { original, canonical: "", language };

  // Existing English wording already has a STEP 11 contract. Preserve it unless
  // the question uses Chinese sentence structure or a mixed-language alias.
  if (language === "en" && !/[\p{Script=Han}]/u.test(q)) {
    return { original, canonical: q, language };
  }

  if (/^为什么$/.test(q)) return { original, canonical: "Why?", language };
  if (/^(那|这些|它们|还有)/.test(q)) return { original, canonical: "What about those?", language };
  if (hasTerm(q, "analytical")) return { original, canonical: `Analyze: ${q}`, language };
  if (hasTerm(q, "negation")) return { original, canonical: q, language };
  if (/(去年|今年|本月|上个月|明年|以前|以后|同时|并且)/.test(q)) return { original, canonical: q, language };

  const stage = findStage(q);
  const risk = riskLevel(q);
  const riskPhrase = risk && risk !== "Critical" ? `${risk} Risk` : risk;
  const customer = entity(q, CUSTOMER_PATTERNS);
  const manager = entity(q, MANAGER_PATTERNS);
  const mcrId = q.match(/\bBM-\d+\b/i)?.[0];
  const list = /(哪些|哪几个|有哪些|显示|列出|查找|项目)/.test(q);
  const count = /(多少|数量|总数)/.test(q);
  if (count && (customer || manager || mcrId)) return { original, canonical: q, language };
  if (count && !risk && !/^(?:有多少(?:个)?项目|项目总数(?:是多少)?|一共有多少(?:个)?项目)$/.test(q)) return { original, canonical: q, language };
  if (risk && hasTerm(q, "overdue")) return { original, canonical: q, language };
  if (risk && hasTerm(q, "hold")) return { original, canonical: q, language };

  let canonical: string | undefined;
  if (customer && manager) canonical = undefined;
  else if (customer && risk) canonical = `Show ${riskPhrase} projects for customer ${customer}`;
  else if (manager && risk) canonical = `Show ${riskPhrase} projects managed by ${manager}`;
  else if (customer) canonical = `Show projects for customer ${customer}`;
  else if (manager) canonical = `Show projects managed by ${manager}`;
  else if (mcrId && list) canonical = `Show project with MCR-ID ${mcrId}`;
  else if (hasTerm(q, "risk") && hasTerm(q, "customer") && /(最多|排名)/.test(q)) canonical = "Which customers have the most high-risk projects?";
  else if (hasTerm(q, "manager") && risk === "Critical" && /(哪些|排名)/.test(q)) canonical = "Which project managers own Critical projects?";
  else if (/(主要风险原因|主要风险驱动|风险原因|risk drivers?)/i.test(q)) canonical = "What are the main risk drivers?";
  else if (count && risk) canonical = `How many projects are ${riskPhrase}?`;
  else if (/^(?:有多少(?:个)?项目|项目总数(?:是多少)?|一共有多少(?:个)?项目)$/.test(q)) canonical = "How many projects are there?";
  else if (list && hasTerm(q, "overdue")) canonical = stage ? `Which projects have overdue ${stage}?` : "Which projects are overdue?";
  else if (list && hasTerm(q, "hold")) canonical = "Which projects are on HOLD?";
  else if (list && hasTerm(q, "ecu") && (/(包含|有|哪些)/.test(q) || hasTerm(q, "component"))) canonical = "Which projects contain ECU?";
  else if (list && risk) canonical = `Which projects are ${riskPhrase}?`;

  return { original, canonical: clean(canonical ?? q), language };
}
