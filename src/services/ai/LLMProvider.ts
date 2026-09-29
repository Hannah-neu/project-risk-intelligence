import type { AIProvider } from "./AIProvider";
import type { AIAnswer, AIConversationContext } from "./types";
import { enterpriseErrorMessage, invokeEnterpriseLLM, type EnterpriseQuestionContext, type EnterpriseReply } from "./enterpriseClient";

export class LLMProvider implements AIProvider {
  constructor(private readonly invokeBackend: (question: string, language: string, context?: EnterpriseQuestionContext) => Promise<EnterpriseReply> = invokeEnterpriseLLM) {}

  async askQuestion(question = "", context?: AIConversationContext): Promise<AIAnswer> {
    const language = context?.language ?? "en";
    try {
      const reply = await this.invokeBackend(question, language, context);
      return { content: reply.text, answerType: "text", provider: "llm", language, items: [], metadata: { model: reply.model, inputTokens: reply.inputTokens, outputTokens: reply.outputTokens } };
    } catch (error) {
      throw new Error(enterpriseErrorMessage(error));
    }
  }
}
