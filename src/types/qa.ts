import type { RiskLevel } from "./dashboard";

export interface QaProject {
  id: number;
  projectName: string;
  mcrId: string;
  projectStatus: string;
  riskLevel: RiskLevel;
  riskScore: number;
  primaryRiskDriver: string | null;
}

export type QaAnswer =
  | { type: "metric"; summary: string; value: number }
  | { type: "project_list"; summary: string; total: number; projects: QaProject[] }
  | { type: "grouped_summary"; summary: string; groups: { label: string; count: number }[] }
  | { type: "text_summary"; summary: string; items: string[] }
  | { type: "unsupported_question"; summary: string; suggestions: string[] };
