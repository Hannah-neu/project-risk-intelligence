import type { NormalizedQuestion } from "./QuestionNormalizer";
import { parseStructuredQuery, type StructuredQuery } from "./StructuredQuery";

export interface QueryParseAssessment {
  query: StructuredQuery | null;
  confidence: "high" | "low";
  unresolvedTerms: string[];
  requiresAnalysis: boolean;
}

const NARRATIVE_CUES: Array<[string, RegExp]> = [
  ["open-ended status", /情况(?:怎么样|如何)|表现(?:怎么样|如何)|最近怎么样|how (?:is|are).*(?:doing|going)/iu],
  ["qualitative amount", /多不多|算不算多|a lot|relatively (?:many|few)/iu],
  ["analysis", /分析|总结|归纳|看看|看一下|analy[sz]e|summari[sz]e|review/iu],
  ["explanation", /为什么|为何|原因|why\b|reason/iu],
  ["attention", /值得关注|需要关注|重点关注|attention|prioriti[sz]e/iu],
  ["pattern or trend", /共同(?:风险|问题)|主要(?:风险|问题)|模式|趋势|patterns?|trends?|main (?:risks?|problems?)/iu],
  ["qualitative risk", /风险(?:比较|较|很)?(?:高|低)|(?:higher|lower|elevated) risk/iu],
  ["owner and risk details", /(?:负责人|项目经理|owners?|project managers?).*?(?:有哪些?风险|风险(?:原因|详情)|risk (?:reasons?|details?))|(?:有哪些?风险|风险(?:原因|详情)|risk (?:reasons?|details?)).*?(?:负责人|项目经理|owners?|project managers?)/iu],
  ["anomaly", /异常|是否正常|unusual|abnormal/iu]
];

/**
 * The deterministic parser is intentionally permissive, so this gate verifies
 * that it captured the complete user intent before SQLite execution. A partial
 * parse is never silently treated as the whole question.
 */
export function assessStructuredQuery(normalized: NormalizedQuestion): QueryParseAssessment {
  const query = parseStructuredQuery(normalized);
  if (!query) return { query: null, confidence: "low", unresolvedTerms: [], requiresAnalysis: false };

  const text = normalized.original;
  const unresolvedTerms = NARRATIVE_CUES.filter(([, pattern]) => pattern.test(text)).map(([label]) => label);
  const requiresAnalysis = unresolvedTerms.length > 0;

  const conjunctions = text.match(/\b(?:and|also|plus)\b|并且|同时|而且|以及/giu)?.length ?? 0;
  if (conjunctions > 0 && query.filters.length < 2) unresolvedTerms.push("uncaptured combined condition");

  return {
    query,
    confidence: unresolvedTerms.length === 0 ? "high" : "low",
    unresolvedTerms: [...new Set(unresolvedTerms)],
    requiresAnalysis
  };
}

export function confidentStructuredQuery(normalized: NormalizedQuestion): StructuredQuery | null {
  const assessment = assessStructuredQuery(normalized);
  return assessment.confidence === "high" ? assessment.query : null;
}
