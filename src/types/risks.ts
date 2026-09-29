import type { RiskLevel } from "./dashboard";
import type { ProjectFilterOptions } from "./projects";

export interface RiskOverviewQuery {
  search?: string;
  customer?: string;
  projectManager?: string;
  acqManager?: string;
  engineSegment?: string;
  productCategory?: string;
  emission?: string;
  projectType?: string;
  system?: string;
  projectStatus?: string;
  riskLevel?: string;
  limit: number;
  offset: number;
}

export interface RiskRegisterSelections {
  customer: string;
  projectManager: string;
  acqManager: string;
  engineSegment: string;
  productCategory: string;
  emission: string;
  projectType: string;
  system: string;
  projectStatus: string;
  riskLevel: string;
}

export interface RiskBreakdownItem {
  label: string;
  critical: number;
  high: number;
  medium: number;
  low: number;
  total: number;
  exposure: number;
}

export interface RiskBreakdowns {
  customers: RiskBreakdownItem[];
  projectManagers: RiskBreakdownItem[];
  systems: RiskBreakdownItem[];
  productCategories: RiskBreakdownItem[];
}

export interface RiskTableProject {
  id: number;
  mcrId: string;
  projectName: string;
  customer: string;
  projectManager: string;
  riskScore: number;
  riskLevel: RiskLevel;
  primaryRiskDriver: string | null;
  nextMilestone: string;
  nextMilestoneDate: string | null;
}

export interface RiskOverviewData {
  totalProjects: number;
  registerTotal: number;
  filters: ProjectFilterOptions;
  critical: number;
  high: number;
  medium: number;
  low: number;
  breakdowns: RiskBreakdowns;
  projects: RiskTableProject[];
}
