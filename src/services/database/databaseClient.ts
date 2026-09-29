import { invoke } from "@tauri-apps/api/core";
import type { DatabaseStatus, ImportHistoryItem, ImportPreview, ImportResult, McrApplyResult, McrPreview, ProjectListExportResult, SyncHistoryItem } from "../../types/database";
import type { DashboardData } from "../../types/dashboard";
import type { InsightOverviewData } from "../../types/insights";
import type { ProjectDetail, ProjectListQuery, ProjectListResponse } from "../../types/projects";
import type { StructuredQuery } from "../ai/StructuredQuery";
import type { QaAnswer } from "../../types/qa";
import type { RiskOverviewData, RiskOverviewQuery } from "../../types/risks";
import type { AnalysisLanguage, ProjectAIAnalysis } from "../../types/projectAnalysis";

export function getDatabaseStatus(): Promise<DatabaseStatus> {
  return invoke<DatabaseStatus>("get_database_status");
}

export function exportProjectList(filePath: string): Promise<ProjectListExportResult> {
  return invoke<ProjectListExportResult>("export_project_list", { filePath });
}

export function getDashboardData(): Promise<DashboardData> {
  return invoke<DashboardData>("get_dashboard_data");
}

export function getInsightOverview(): Promise<InsightOverviewData> {
  return invoke<InsightOverviewData>("get_insight_overview");
}

export function askProjectData(question: string): Promise<QaAnswer> {
  return invoke<QaAnswer>("ask_project_data", { question });
}

export function askStructuredProjectData(query: StructuredQuery): Promise<QaAnswer> {
  return invoke<QaAnswer>("ask_structured_project_data", { query });
}

export function explainSystemQuestion(intent: string, question: string, language: "zh" | "en"): Promise<string> {
  return invoke<string>("explain_system_question", { intent, question, language });
}

export function getDatasetVersion(): Promise<string> {
  return invoke<string>("get_dataset_version");
}

export function getProjects(query: ProjectListQuery): Promise<ProjectListResponse> {
  return invoke<ProjectListResponse>("get_projects", { query });
}

export function getProjectDetail(projectId: number): Promise<ProjectDetail> {
  return invoke<ProjectDetail>("get_project_detail", { projectId });
}

export function loadProjectAIAnalysis(projectId: number): Promise<ProjectAIAnalysis | null> {
  return invoke<ProjectAIAnalysis | null>("load_project_ai_analysis", { projectId });
}

export function generateProjectAIAnalysis(projectId: number, language: AnalysisLanguage): Promise<ProjectAIAnalysis> {
  return invoke<ProjectAIAnalysis>("generate_project_ai_analysis", { projectId, language });
}

export function getRiskOverview(query: RiskOverviewQuery): Promise<RiskOverviewData> {
  return invoke<RiskOverviewData>("get_risk_overview", { query });
}

export function getSetting(key: string): Promise<string | null> {
  return invoke<string | null>("get_setting", { key });
}

export function setSetting(key: string, value: string): Promise<void> {
  return invoke<void>("set_setting", { key, value });
}

export function previewExcel(filePath: string): Promise<ImportPreview> {
  return invoke<ImportPreview>("preview_excel", { filePath });
}

export function replaceExcelPreview(previewToken: string): Promise<ImportResult> {
  return invoke<ImportResult>("replace_excel_preview", { previewToken });
}

export function cancelImportPreview(previewToken: string): Promise<void> {
  return invoke<void>("cancel_import_preview", { previewToken });
}

export function getImportHistory(limit = 10): Promise<ImportHistoryItem[]> {
  return invoke<ImportHistoryItem[]>("get_import_history", { limit });
}

export function getSyncHistory(limit = 10): Promise<SyncHistoryItem[]> {
  return invoke<SyncHistoryItem[]>("get_sync_history", { limit });
}

export function previewMcrExcel(filePath: string): Promise<McrPreview> {
  return invoke<McrPreview>("preview_mcr_excel", { filePath });
}

export function applyMcrPreview(previewToken: string): Promise<McrApplyResult> {
  return invoke<McrApplyResult>("apply_mcr_preview", { previewToken });
}

export function cancelMcrPreview(previewToken: string): Promise<void> {
  return invoke<void>("cancel_mcr_preview", { previewToken });
}
