import type { AIProvider } from "./AIProvider";
import { LLMProvider } from "./LLMProvider";
import { LocalQueryProvider } from "./LocalQueryProvider";
import { defaultAISettings, type AIProviderKind } from "./types";

const providers: Record<AIProviderKind, AIProvider> = {
  local: new LocalQueryProvider(),
  llm: new LLMProvider()
};

export function getAIProvider(kind: AIProviderKind = defaultAISettings.provider): AIProvider {
  return providers[kind];
}
