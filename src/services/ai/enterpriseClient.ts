import { invoke } from "@tauri-apps/api/core";
import type { StructuredQuery } from "./StructuredQuery";

export interface EnterpriseConfig {
  provider: "local" | "llm";
  endpoint: string;
  model: string;
  maxTokens: number;
  anthropicVersion: string;
  hasApiKey: boolean;
}

export interface EnterpriseConfigInput extends Omit<EnterpriseConfig, "hasApiKey"> {
  apiKey?: string;
  clearApiKey?: boolean;
}

export interface EnterpriseReply {
  text: string;
  model: string;
  inputTokens?: number;
  outputTokens?: number;
}

export function enterpriseErrorMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") return error.message;
  return error instanceof Error ? error.message : "The Enterprise LLM request could not be completed.";
}

export const getEnterpriseConfig = () => invoke<EnterpriseConfig>("get_enterprise_config");
export const saveEnterpriseConfig = (config: EnterpriseConfigInput) => invoke<EnterpriseConfig>("save_enterprise_config", { config });
export const testEnterpriseConnection = () => invoke<{ success: boolean; message: string }>("test_enterprise_connection");
export interface EnterpriseQuestionContext {
  structuredQuery?: StructuredQuery;
  lastProjectIds?: number[];
  datasetVersion?: string;
  recentMessages?: { role: "user" | "assistant"; content: string }[];
  requiresCurrentDataset?: boolean;
  directConversation?: boolean;
}

export const invokeEnterpriseLLM = (question: string, language: string, context?: EnterpriseQuestionContext) => invoke<EnterpriseReply>("invoke_enterprise_llm", { request: {
  question, language,
  structuredQuery: context?.structuredQuery,
  lastProjectIds: context?.lastProjectIds?.slice(0, 3) ?? [],
  datasetVersion: context?.datasetVersion,
  requiresCurrentDataset: context?.requiresCurrentDataset,
  directConversation: context?.directConversation,
  recentMessages: context?.recentMessages?.slice(-8).map((item) => ({ role: item.role, content: item.content.slice(0, 600) })) ?? []
} });
