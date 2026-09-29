import type { RiskLevel } from "./dashboard";

export interface ProjectListQuery {
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
  sortBy?: string;
  sortDirection?: "asc" | "desc";
  limit: number;
  offset: number;
}

export interface ProjectListItem {
  id: number;
  mcrId: string;
  projectName: string;
  customer: string;
  projectManager: string;
  system: string;
  productCategory: string;
  projectStatus: string;
  riskLevel: RiskLevel;
  riskScore: number;
  nextMilestone: string;
  nextMilestoneDate: string | null;
}

export interface ProjectFilterOptions {
  customers: string[];
  projectManagers: string[];
  acqManagers: string[];
  engineSegments: string[];
  productCategories: string[];
  emissions: string[];
  projectTypes: string[];
  systems: string[];
  projectStatuses: string[];
  riskLevels: string[];
}

export interface ProjectListResponse {
  items: ProjectListItem[];
  total: number;
  filters: ProjectFilterOptions;
}

export type MilestoneStatus = "Completed on time" | "Completed late" | "Overdue" | "Not completed" | "Not planned";

export interface ProjectMilestoneDetail {
  id: number;
  milestoneType: string;
  plannedDate: string | null;
  actualDate: string | null;
  deltaDays: number | null;
  status: MilestoneStatus;
}

export interface ProjectRiskDetail {
  riskScore: number;
  riskLevel: RiskLevel;
  primaryRiskDriver: string | null;
  riskReasons: string[];
  calculatedAt: string | null;
}

export interface ProjectDetail {
  id: number;
  projectNo: number | null;
  mcrId: string;
  projectName: string;
  customer: string | null;
  pId: string | null;
  acqManager: string | null;
  projectManager: string | null;
  engineSegment: string | null;
  productCategory: string | null;
  emission: string | null;
  apic: string | null;
  projectType: string | null;
  system: string | null;
  components: string | null;
  newReleasedComponents: string | null;
  hasInjector: boolean;
  hasPump: boolean;
  hasRail: boolean;
  hasEcu: boolean;
  hasDnox: boolean;
  otherComponent: string | null;
  projectStatus: string | null;
  bSample: string | null;
  cSample: string | null;
  dSample: string | null;
  sourceFile: string;
  sourceRow: number;
  updatedAt: string;
  milestones: ProjectMilestoneDetail[];
  risk: ProjectRiskDetail;
}
