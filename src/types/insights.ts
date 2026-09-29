import type { RiskLevel } from "./dashboard";
import type { RiskBreakdownItem } from "./risks";

export type InsightType = "warning" | "milestone" | "portfolio" | "recommendation" | "summary";

export interface RuleInsight {
  id: number;
  type: InsightType;
  title: string;
  description: string;
  severity: "high" | "medium" | "low" | null;
  createdAt: string;
}

export interface InsightPortfolioSummary {
  totalProjects: number;
  highCriticalProjects: number;
  overdueMilestones: number;
  completedLateMilestones: number;
}

export interface InsightRiskDriver {
  driver: string;
  projects: number;
  maxRiskScore: number;
}

export interface InsightAttentionProject {
  id: number;
  mcrId: string;
  projectName: string;
  riskScore: number;
  riskLevel: RiskLevel;
  reason: string;
  nextMilestone: string;
  nextMilestoneDate: string | null;
  owner: string;
}

export interface InsightMilestoneAnalysis {
  milestone: string;
  overdue: number;
  completedLate: number;
  completedOnTime: number;
  averageDelayDays: number;
}

export interface InsightOverviewData {
  generatedAt: string | null;
  summary: InsightPortfolioSummary;
  insights: RuleInsight[];
  topRiskDrivers: InsightRiskDriver[];
  attentionProjects: InsightAttentionProject[];
  milestoneAnalysis: InsightMilestoneAnalysis[];
  riskByCustomer: RiskBreakdownItem[];
  riskByProjectManager: RiskBreakdownItem[];
}
