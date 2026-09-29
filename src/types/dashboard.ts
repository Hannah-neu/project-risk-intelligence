export type RiskLevel = "Critical" | "High" | "Medium" | "Low";

export interface ProjectSummary {
  id: number;
  mcrId: string;
  projectName: string;
  customer: string;
  projectManager: string;
  phase: string;
  riskLevel: RiskLevel;
  riskScore: number;
  projectStatus: string;
  lastUpdate: string;
  nextMilestone: string;
  nextMilestoneDate: string | null;
}

export interface DashboardStat {
  label: string;
  value: number;
  detail: string;
  tone: "primary" | "danger" | "warning" | "success";
  icon: "projects" | "onTrack" | "critical" | "overdue";
}

export interface Insight {
  id: number;
  type: "warning" | "milestone" | "portfolio" | "recommendation" | "summary";
  title: string;
  description: string;
}

export interface DashboardData {
  totalProjects: number;
  onTrackProjects: number;
  criticalProjects: number;
  overdueProjects: number;
  sourceFile: string | null;
  projects: ProjectSummary[];
  insights: Insight[];
}
