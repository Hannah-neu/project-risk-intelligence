import type { QaProject } from "../../types/qa";
import type { QuestionLanguage } from "./QuestionNormalizer";
import type { StructuredQuery } from "./StructuredQuery";

export type AIProviderKind = "local" | "llm";

export interface AIConversationContext {
  language?: QuestionLanguage;
  structuredQuery?: StructuredQuery;
  recentMessages?: { role: "user" | "assistant"; content: string }[];
  lastIntent?: AIAnswer["answerType"];
  lastProjectIds?: number[];
  datasetVersion?: string;
  requiresCurrentDataset?: boolean;
  directConversation?: boolean;
}

export interface AISettings {
  provider: AIProviderKind;
  endpoint?: string;
  model?: string;
  apiKey?: string;
  maxTokens?: number;
  temperature?: number;
}

export const defaultAISettings: Readonly<AISettings> = { provider: "local" };

type AnswerBase = {
  content: string;
  provider: AIProviderKind;
  language?: QuestionLanguage;
  referencedProjectIds?: number[];
  metadata?: Record<string, unknown>;
};

export type AIAnswer = AnswerBase & (
  | { answerType: "metric"; value: number }
  | { answerType: "project_list"; total: number; projects: QaProject[] }
  | { answerType: "grouped_summary"; groups: { label: string; count: number }[] }
  | { answerType: "text"; items: string[] }
  | { answerType: "unsupported"; suggestions: string[] }
);
