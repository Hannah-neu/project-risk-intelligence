export type AnalysisLanguage = "en" | "zh";

export interface ProjectAIAnalysis {
  id: number;
  projectId: number;
  projectMcrId: string;
  datasetVersion: string;
  model: string;
  language: AnalysisLanguage;
  content: string;
  riskScoreAtGeneration: number;
  riskLevelAtGeneration: string;
  createdAt: string;
  outdated: boolean;
}
