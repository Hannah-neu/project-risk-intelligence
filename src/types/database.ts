export interface DatabaseStatus {
  path: string;
  schemaVersion: number;
  databaseSizeBytes: number;
  totalProjects: number;
  totalMilestones: number;
  totalRisks: number;
  totalInsights: number;
  totalImports: number;
}

export interface ProjectListExportResult {
  projectCount: number;
  filePath: string;
}

export interface ImportResult {
  fileName: string;
  totalProjects: number;
  totalMilestones: number;
  importedAt: string;
  highRiskProjects: number;
  warningCount: number;
  formulaErrorCount: number;
}

export type ImportIssueSeverity = "error" | "warning" | "info";

export interface ImportIssue {
  severity: ImportIssueSeverity;
  category: string;
  row: number | null;
  field: string | null;
  originalValue: string | null;
  normalizedValue: string | null;
  issue: string;
}

export interface ImportPreview {
  previewToken: string;
  fileName: string;
  projectsDetected: number;
  validRecords: number;
  errorCount: number;
  warningCount: number;
  infoCount: number;
  duplicateMcrIds: number;
  formulaErrors: number;
  normalizedDates: number;
  normalizedComponentLists: number;
  issues: ImportIssue[];
}

export interface ImportHistoryItem {
  id: number;
  fileName: string;
  importTime: string;
  totalProjects: number;
  totalMilestones: number;
  status: "success" | "failed";
  errorMessage: string | null;
  fileSize: number | null;
  warningCount: number;
  formulaErrorCount: number;
}

export interface SyncHistoryItem {
  kind: "project_list" | "mcr";
  id: number;
  fileName: string;
  eventTime: string;
  status: "success" | "failed";
  projectCount: number | null;
  updatedMilestones: number | null;
  projectsAffected: number | null;
  warningCount: number;
}

export interface ImportFailure {
  code: string;
  message: string;
  details: string[];
}

export interface McrUpdateItem {
  projectId: number;
  mcrId: string;
  projectName: string;
  milestoneType: string;
  currentPlanDate: string | null;
  currentPlanSource: string;
  newPlanDate: string;
  contributingRows: number[];
  changed: boolean;
}

export interface McrPreview {
  previewToken: string;
  fileName: string;
  rowsDetected: number;
  masterIdsDetected: number;
  matchedProjects: number;
  unmatchedMasterIds: number;
  milestoneDatesDetected: number;
  projectsAffected: number;
  warningCount: number;
  errorCount: number;
  items: McrUpdateItem[];
  issues: ImportIssue[];
}

export interface McrApplyResult {
  fileName: string;
  updatedMilestones: number;
  projectsAffected: number;
  appliedAt: string;
}
