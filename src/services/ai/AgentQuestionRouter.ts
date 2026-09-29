import type { AIProvider } from "./AIProvider";
import { getEnterpriseConfig } from "./enterpriseClient";
import { getAIProvider } from "./providerFactory";
import { normalizeQuestion } from "./QuestionNormalizer";
import { QuestionRouter } from "./QuestionRouter";
import type { AIAnswer, AIConversationContext } from "./types";

/**
 * STEP 15 entry point. Natural-language understanding belongs to the Enterprise
 * agent whenever it is configured. The deterministic router remains an offline
 * compatibility path and a safe execution model, not the language gate.
 */
export class AgentQuestionRouter {
  constructor(
    private readonly fallback = new QuestionRouter(),
    private readonly llm: AIProvider = getAIProvider("llm"),
    private readonly llmAvailable: () => Promise<boolean> = async () => {
      try {
        const config = await getEnterpriseConfig();
        return config.provider === "llm" && config.hasApiKey;
      } catch {
        return false;
      }
    },
  ) {}

  resetConversation() {
    this.fallback.resetConversation();
  }

  async askQuestion(question: string, context?: AIConversationContext): Promise<AIAnswer> {
    const normalized = normalizeQuestion(question);
    if (await this.llmAvailable()) {
      return this.llm.askQuestion(normalized.original, {
        ...context,
        language: normalized.language,
        structuredQuery: undefined,
        requiresCurrentDataset: true,
      });
    }
    return this.fallback.askQuestion(question, context);
  }
}
