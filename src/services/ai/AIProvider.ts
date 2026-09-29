import type { AIAnswer, AIConversationContext } from "./types";

export interface AIProvider {
  askQuestion(question: string, context?: AIConversationContext): Promise<AIAnswer>;
}
