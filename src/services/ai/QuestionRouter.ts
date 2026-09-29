import type { AIProvider } from "./AIProvider";
import { normalizeQuestion, type NormalizedQuestion } from "./QuestionNormalizer";
import { getAIProvider } from "./providerFactory";
import { hasTerm, terminology } from "./terminology";
import { getEnterpriseConfig } from "./enterpriseClient";
import { classifySystemQuestion, type SystemQuestionIntent } from "./SystemQuestionClassifier";
import { SystemExplanationProvider } from "./SystemExplanationProvider";
import type { AIAnswer, AIConversationContext, AIProviderKind } from "./types";
import { parseStructuredQuery, type QueryOperation, type StructuredQuery } from "./StructuredQuery";
import { extractPersonName, hasUnresolvedPersonMention } from "./personName";
import { getDatasetVersion } from "../../services/database/databaseClient";
import { isFollowUp, resolveFollowUp, stateFromAnswer, type ConversationQueryState } from "./FollowUpResolver";
import { interpretSemanticQuestion, type SemanticPlan } from "./SemanticInterpreter";
import { assessStructuredQuery } from "./QueryCompletenessGate";

export type QuestionCategory =
  | SystemQuestionIntent
  | "factual_query"
  | "project_filter"
  | "aggregation"
  | "analytical_question"
  | "conversational_followup"
  | "unsupported";

export interface QuestionRoute {
  category: QuestionCategory;
  provider: AIProviderKind | null;
}

const LIST_START = /^(which projects|show projects|list projects)\b/;
const COUNT_START = /^(how many |count )/;
const RISK_OWNER_RANKING = /(?:风险(?:比较|较|很)?大|风险最高|风险暴露|higher risk|highest risk|risk exposure).*?(?:工程师|负责人|项目经理|engineers?|owners?|project managers?)|(?:工程师|负责人|项目经理|engineers?|owners?|project managers?).*?(?:风险(?:比较|较|很)?大|风险最高|风险暴露|higher risk|highest risk|risk exposure)/iu;
const isRiskOwnerRanking = (question: string) => RISK_OWNER_RANKING.test(question);
const needsConversationalOrchestration = (question: string) => /\b(?:compare|versus|vs\.?|relationship|correlation|explain together)\b|比较|对比|相比|分别|关联|综合分析/iu.test(question);

function classifyCanonical(question: string, context?: AIConversationContext): QuestionCategory {
  const q = question.toLowerCase().replace(/[?？.。!！]+$/, "").trim();
  if (!q || q.length > 500) return "unsupported";

  // The STEP 11 classifier cannot safely express negation or combined constraints.
  if (hasTerm(q, "negation")) return "unsupported";
  if (context?.lastIntent && terminology.followup.some((alias) => q.startsWith(alias))) return "conversational_followup";
  if (hasTerm(q, "analytical") || /\b(?:unusual|attention)\b|值得关注|需要关注/.test(q)) return "analytical_question";
  if (isRiskOwnerRanking(q)) return "analytical_question";
  if (/\b(and|or|before|after|last|next|this|month|year|since|between)\b|以及|并且|去年|今年|本月/.test(q)) return "unsupported";

  if (/main risk drivers?|top risk drivers?/.test(q)) return "aggregation";
  if (/customer/.test(q) && /(most high-risk|most high risk)/.test(q)) return "aggregation";
  if (/project manager/.test(q) && /critical/.test(q) && !/managed by/.test(q)) return "aggregation";

  if (/^show (critical|high risk|medium risk|low risk) projects (for customer|managed by) \S/i.test(q)) return "project_filter";
  if (/^show project with mcr-id \S/i.test(q)) return "project_filter";
  if (/^(show projects for customer |projects for customer |show projects of customer |which projects belong to customer |which projects are for customer )\S/.test(q)) return "project_filter";
  if (/^(show projects managed by |projects managed by |show projects for project manager |which projects are managed by )\S/.test(q)) return "project_filter";

  const list = LIST_START.test(q);
  const count = COUNT_START.test(q);
  if (list) {
    const newComponents = /newly released|new released/.test(q);
    const ecu = /(contain|have)/.test(q) && hasTerm(q, "ecu");
    const overdue = hasTerm(q, "overdue");
    const highest = /highest risk score|highest risk/.test(q);
    const hold = hasTerm(q, "hold");
    const critical = hasTerm(q, "critical");
    const high = hasTerm(q, "high");
    const medium = hasTerm(q, "medium");
    const low = hasTerm(q, "low");
    const matches = [newComponents, ecu && !newComponents, overdue, highest, hold, critical, high, medium, low].filter(Boolean).length;
    if (matches === 1) return "project_filter";
    return "unsupported";
  }

  if (count && ["critical", "high", "medium", "low"].filter((term) => hasTerm(q, term as "critical" | "high" | "medium" | "low")).length === 1) {
    return "factual_query";
  }
  if (["how many projects are there", "how many projects", "count projects", "total projects"].includes(q)) return "factual_query";
  return "unsupported";
}

function structuredCategory(operation: QueryOperation): QuestionCategory {
  if (["group_by", "max", "min", "top", "sort", "distinct"].includes(operation)) return "aggregation";
  return ["count", "count_distinct"].includes(operation) ? "factual_query" : "project_filter";
}

export function classifyQuestion(question: string, context?: AIConversationContext): QuestionCategory {
  const system = classifySystemQuestion(question);
  if (system) return system;
  const normalized = normalizeQuestion(question);
  if (isRiskOwnerRanking(normalized.original)) return "analytical_question";
  const assessment = assessStructuredQuery(normalized);
  if (assessment.query && assessment.confidence === "low") return assessment.requiresAnalysis ? "analytical_question" : "unsupported";
  const parsed = assessment.confidence === "high" ? assessment.query : null;
  return parsed ? structuredCategory(parsed.operation) : classifyCanonical(normalized.canonical, context);
}

export class QuestionRouter {
  private conversationState: ConversationQueryState | null = null;
  constructor(
    private readonly localProvider: AIProvider = getAIProvider("local"),
    private readonly llmProvider: AIProvider = getAIProvider("llm"),
    private readonly llmAvailable: () => Promise<boolean> = async () => {
      try { const config = await getEnterpriseConfig(); return config.provider === "llm" && config.hasApiKey; }
      catch { return false; }
    },
    private readonly explanationProvider: SystemExplanationProvider = new SystemExplanationProvider(),
    private readonly readDatasetVersion: () => Promise<string> = getDatasetVersion,
    private readonly semanticInterpreter: (question: string) => Promise<SemanticPlan> = interpretSemanticQuestion
  ) {}

  resetConversation() { this.conversationState = null; }

  route(question: string, context?: AIConversationContext): QuestionRoute {
    return this.routeNormalized(normalizeQuestion(question), context);
  }

  private routeNormalized(normalized: NormalizedQuestion, context?: AIConversationContext): QuestionRoute {
    const system = classifySystemQuestion(normalized.original);
    if (system) return { category: system, provider: "local" };
    if (isFollowUp(normalized.original)) {
      const followup = resolveFollowUp(normalized.original, this.conversationState);
      if (followup?.kind === "local") return { category: "conversational_followup", provider: "local" };
      if (followup?.kind === "analytical") return { category: "conversational_followup", provider: "llm" };
      if (context?.lastProjectIds?.length) return { category: "conversational_followup", provider: "llm" };
      return { category: "conversational_followup", provider: null };
    }
    if (isRiskOwnerRanking(normalized.original)) return { category: "analytical_question", provider: "llm" };
    const assessment = assessStructuredQuery(normalized);
    const parsed = assessment.confidence === "high" ? assessment.query : null;
    if (parsed) return { category: structuredCategory(parsed.operation), provider: "local" };
    if (assessment.query && assessment.confidence === "low") {
      return { category: assessment.requiresAnalysis ? "analytical_question" : "unsupported", provider: null };
    }
    const category = classifyCanonical(normalized.canonical, context);
    if (category === "analytical_question" || category === "conversational_followup") {
      return { category, provider: "llm" };
    }
    if (hasUnresolvedPersonMention(normalized.original, [])) return { category: "unsupported", provider: null };
    if (category === "unsupported") return { category, provider: null };
    return { category, provider: "local" };
  }

  async askQuestion(question: string, context?: AIConversationContext): Promise<AIAnswer> {
    const normalized = normalizeQuestion(question);
    const systemIntent = classifySystemQuestion(normalized.original);
    if (systemIntent) return this.explanationProvider.answer(systemIntent, normalized.original, normalized.language);
    const followup = isFollowUp(normalized.original) ? resolveFollowUp(normalized.original, this.conversationState) : null;
    if (followup && this.conversationState) {
      const currentVersion = await this.readDatasetVersion();
      if (currentVersion !== this.conversationState.datasetVersion) {
        this.resetConversation();
        return this.clarification(normalized.language, true);
      }
    }
    if (followup?.kind === "clarify" && !context?.lastProjectIds?.length) return this.clarification(normalized.language);
    const route = this.routeNormalized(normalized, context);
    const assessment = assessStructuredQuery(normalized);
    if (route.provider === null && needsConversationalOrchestration(normalized.original) && await this.llmAvailable()) {
      const result = await this.llmProvider.askQuestion(normalized.original, {
        ...context,
        language: normalized.language,
        structuredQuery: undefined,
        datasetVersion: await this.readDatasetVersion(),
        requiresCurrentDataset: true
      });
      this.debugExecution("llm_semantic", "conversational_orchestration", null, result, "llm");
      return result;
    }
    if (route.provider === null && await this.llmAvailable()) {
      try {
        const semantic = await this.semanticInterpreter(normalized.original);
        const result = await this.executeSemanticPlan(semantic, normalized, context);
        if (result) return result;
      } catch {
        // A failed or invalid semantic plan must remain controlled; never broaden the query.
      }
    }
    const structuredQuery = followup?.kind === "local" ? followup.query : route.provider === "local" && !classifySystemQuestion(normalized.original) ? (assessment.confidence === "high" ? assessment.query : null) : undefined;
    const providerContext = { ...context, language: normalized.language, structuredQuery: structuredQuery ?? undefined };
    const usesReference = route.category === "conversational_followup" || needsConversationalOrchestration(normalized.original) || /\b(first|those|these|this project|that project|previous|it)\b|第一个|这个项目|该项目|这些项目|刚才/.test(normalized.original.toLowerCase());
    const analysisScope = followup?.kind === "analytical" ? followup.query : route.provider === "llm" ? parseStructuredQuery(normalized, true) : null;
    const llmContextBase = { ...providerContext, structuredQuery: analysisScope ?? undefined, datasetVersion: followup?.kind === "analytical" ? this.conversationState?.datasetVersion : providerContext.datasetVersion, requiresCurrentDataset: followup?.kind === "analytical" };
    const llmContext = usesReference ? llmContextBase : { ...llmContextBase, lastProjectIds: undefined, recentMessages: undefined, datasetVersion: undefined };
    if (typeof window !== "undefined" && window.location.hostname === "localhost") {
      console.debug("[AI structured query]", {
        parserSource: "deterministic",
        normalizedQuestion: normalized.canonical,
        parseConfidence: assessment.confidence,
        unresolvedTerms: assessment.unresolvedTerms,
        extractedEntities: structuredQuery?.filters.map(({ field, value }) => ({ field, value })) ?? (extractPersonName(normalized.original) ? [{ field: "project_manager", value: extractPersonName(normalized.original) }] : []),
        structuredQuery: structuredQuery ?? null,
        filters: structuredQuery?.filters ?? [],
        executedQueryType: route.provider === "local" ? structuredQuery ? "structured_sqlite" : "legacy_sqlite" : route.provider === "llm" ? "existing_llm" : "unsupported"
      });
    }
    const debug = (provider: "local" | "llm") => {
      if (typeof window !== "undefined" && window.location.hostname === "localhost") console.debug("[AI route]", { provider, intent: route.category });
    };
    if (route.provider === "local") {
      debug("local");
      const result = await this.localProvider.askQuestion(normalized.canonical, providerContext);
      if (structuredQuery) this.conversationState = stateFromAnswer(structuredQuery, result);
      else this.conversationState = null;
      this.debugExecution("deterministic", route.category, structuredQuery ?? null, result, "local");
      return result;
    }
    if (route.provider === "llm" && await this.llmAvailable()) {
      debug("llm");
      const result = await this.llmProvider.askQuestion(normalized.original, llmContext);
      this.debugExecution("deterministic", route.category, analysisScope, result, "llm");
      return result;
    }
    return {
      content: normalized.language === "zh" ? "暂不支持这个问题。请尝试询问项目数量、受支持的项目筛选或主要风险原因。" : "This question is not supported yet. Try asking for a project count, a supported project list, or a risk-driver summary.",
      answerType: "unsupported",
      provider: "local",
      language: normalized.language,
      suggestions: normalized.language === "zh" ? ["有多少个严重风险项目？", "哪些项目逾期了？", "主要风险原因是什么？"] : ["How many projects are Critical?", "Which projects are overdue?", "What are the main risk drivers?"]
    };
  }

  private clarification(language: "zh" | "en", changed = false): AIAnswer {
    return { content: language === "zh" ? changed ? "项目数据已更新。请重新说明要查询的项目范围。" : "请先说明你指的是哪些项目或哪个上一轮结果。" : changed ? "The project dataset changed. Please restate the project scope." : "Please specify which projects or previous result you mean.", answerType: "unsupported", provider: "local", language, suggestions: language === "zh" ? ["哪些项目是 Critical？"] : ["Which projects are Critical?"] };
  }

  private async executeSemanticPlan(plan: SemanticPlan, normalized: NormalizedQuestion, context?: AIConversationContext): Promise<AIAnswer | null> {
    if (plan.intent === "unsupported") return null;
    if (plan.intent === "general_conversation") {
      const result = await this.llmProvider.askQuestion(normalized.original, { language: normalized.language, directConversation: true });
      this.conversationState = null;
      this.debugSemantic(plan, result, "llm");
      return result;
    }
    if (!plan.query) return null;
    if (plan.intent === "analytical_question") {
      const result = await this.llmProvider.askQuestion(normalized.original, {
        ...context,
        language: normalized.language,
        structuredQuery: plan.query,
        datasetVersion: await this.readDatasetVersion(),
        lastProjectIds: undefined,
        recentMessages: undefined
      });
      this.debugSemantic(plan, result, "llm");
      return result;
    }
    const result = await this.localProvider.askQuestion(normalized.canonical, { ...context, language: normalized.language, structuredQuery: plan.query });
    this.conversationState = stateFromAnswer(plan.query, result);
    this.debugSemantic(plan, result, "local");
    return result;
  }

  private debugSemantic(plan: SemanticPlan, result: AIAnswer, provider: "local" | "llm") {
    this.debugExecution("llm_semantic", plan.intent, plan.query, result, provider);
  }

  private debugExecution(parserSource: "deterministic" | "llm_semantic", intent: string, structuredQuery: StructuredQuery | null, result: AIAnswer, provider: "local" | "llm") {
    if (typeof window === "undefined" || window.location.hostname !== "localhost") return;
    const rowCount = result.answerType === "project_list" ? result.total : result.answerType === "metric" ? result.value : result.answerType === "grouped_summary" ? result.groups.length : result.answerType === "text" ? result.items.length : 0;
    console.debug("[AI route]", { parserSource, intent, structuredQuery, validation: "accepted", provider, resultRowCount: rowCount });
  }
}
