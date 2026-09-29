use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseStatus {
    pub path: String,
    pub schema_version: i32,
    pub database_size_bytes: u64,
    pub total_projects: i64,
    pub total_milestones: i64,
    pub total_risks: i64,
    pub total_insights: i64,
    pub total_imports: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub file_name: String,
    pub total_projects: i64,
    pub total_milestones: i64,
    pub imported_at: String,
    pub high_risk_projects: i64,
    pub warning_count: i64,
    pub formula_error_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportHistoryItem {
    pub id: i64,
    pub file_name: String,
    pub import_time: String,
    pub total_projects: i64,
    pub total_milestones: i64,
    pub status: String,
    pub error_message: Option<String>,
    pub file_size: Option<i64>,
    pub warning_count: i64,
    pub formula_error_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncHistoryItem {
    pub kind: String,
    pub id: i64,
    pub file_name: String,
    pub event_time: String,
    pub status: String,
    pub project_count: Option<i64>,
    pub updated_milestones: Option<i64>,
    pub projects_affected: Option<i64>,
    pub warning_count: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardData {
    pub total_projects: i64,
    pub on_track_projects: i64,
    pub critical_projects: i64,
    pub overdue_projects: i64,
    pub source_file: Option<String>,
    pub projects: Vec<DashboardProject>,
    pub insights: Vec<DashboardInsight>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardProject {
    pub id: i64,
    pub mcr_id: String,
    pub project_name: String,
    pub customer: String,
    pub project_manager: String,
    pub phase: String,
    pub risk_level: String,
    pub risk_score: i64,
    pub project_status: String,
    pub last_update: String,
    pub next_milestone: String,
    pub next_milestone_date: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardInsight {
    pub id: i64,
    #[serde(rename = "type")]
    pub insight_type: String,
    pub title: String,
    pub description: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListQuery {
    pub search: Option<String>,
    pub customer: Option<String>,
    pub project_manager: Option<String>,
    pub acq_manager: Option<String>,
    pub engine_segment: Option<String>,
    pub product_category: Option<String>,
    pub emission: Option<String>,
    pub project_type: Option<String>,
    pub system: Option<String>,
    pub project_status: Option<String>,
    pub risk_level: Option<String>,
    pub sort_by: Option<String>,
    pub sort_direction: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListResponse {
    pub items: Vec<ProjectListItem>,
    pub total: i64,
    pub filters: ProjectFilterOptions,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListItem {
    pub id: i64,
    pub mcr_id: String,
    pub project_name: String,
    pub customer: String,
    pub project_manager: String,
    pub system: String,
    pub product_category: String,
    pub project_status: String,
    pub risk_level: String,
    pub risk_score: i64,
    pub next_milestone: String,
    pub next_milestone_date: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFilterOptions {
    pub customers: Vec<String>,
    pub project_managers: Vec<String>,
    pub acq_managers: Vec<String>,
    pub engine_segments: Vec<String>,
    pub product_categories: Vec<String>,
    pub emissions: Vec<String>,
    pub project_types: Vec<String>,
    pub systems: Vec<String>,
    pub project_statuses: Vec<String>,
    pub risk_levels: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetail {
    pub id: i64,
    pub project_no: Option<i64>,
    pub mcr_id: String,
    pub project_name: String,
    pub customer: Option<String>,
    pub p_id: Option<String>,
    pub acq_manager: Option<String>,
    pub project_manager: Option<String>,
    pub engine_segment: Option<String>,
    pub product_category: Option<String>,
    pub emission: Option<String>,
    pub apic: Option<String>,
    pub project_type: Option<String>,
    pub system: Option<String>,
    pub components: Option<String>,
    pub new_released_components: Option<String>,
    pub has_injector: bool,
    pub has_pump: bool,
    pub has_rail: bool,
    pub has_ecu: bool,
    pub has_dnox: bool,
    pub other_component: Option<String>,
    pub project_status: Option<String>,
    pub b_sample: Option<String>,
    pub c_sample: Option<String>,
    pub d_sample: Option<String>,
    pub source_file: String,
    pub source_row: i64,
    pub updated_at: String,
    pub milestones: Vec<ProjectMilestoneDetail>,
    pub risk: ProjectRiskDetail,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMilestoneDetail {
    pub id: i64,
    pub milestone_type: String,
    pub planned_date: Option<String>,
    pub actual_date: Option<String>,
    pub delta_days: Option<i64>,
    pub status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRiskDetail {
    pub risk_score: i64,
    pub risk_level: String,
    pub primary_risk_driver: Option<String>,
    pub risk_reasons: Vec<String>,
    pub calculated_at: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskOverviewQuery {
    pub search: Option<String>,
    pub customer: Option<String>,
    pub project_manager: Option<String>,
    pub acq_manager: Option<String>,
    pub engine_segment: Option<String>,
    pub product_category: Option<String>,
    pub emission: Option<String>,
    pub project_type: Option<String>,
    pub system: Option<String>,
    pub project_status: Option<String>,
    pub risk_level: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskOverviewData {
    pub total_projects: i64,
    pub register_total: i64,
    pub filters: ProjectFilterOptions,
    pub critical: i64,
    pub high: i64,
    pub medium: i64,
    pub low: i64,
    pub breakdowns: RiskBreakdowns,
    pub projects: Vec<RiskTableProject>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskBreakdowns {
    pub customers: Vec<RiskBreakdownItem>,
    pub project_managers: Vec<RiskBreakdownItem>,
    pub systems: Vec<RiskBreakdownItem>,
    pub product_categories: Vec<RiskBreakdownItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskBreakdownItem {
    pub label: String,
    pub critical: i64,
    pub high: i64,
    pub medium: i64,
    pub low: i64,
    pub total: i64,
    pub exposure: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskTableProject {
    pub id: i64,
    pub mcr_id: String,
    pub project_name: String,
    pub customer: String,
    pub project_manager: String,
    pub risk_score: i64,
    pub risk_level: String,
    pub primary_risk_driver: Option<String>,
    pub next_milestone: String,
    pub next_milestone_date: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightOverviewData {
    pub generated_at: Option<String>,
    pub summary: InsightPortfolioSummary,
    pub insights: Vec<RuleInsight>,
    pub top_risk_drivers: Vec<InsightRiskDriver>,
    pub attention_projects: Vec<InsightAttentionProject>,
    pub milestone_analysis: Vec<InsightMilestoneAnalysis>,
    pub risk_by_customer: Vec<RiskBreakdownItem>,
    pub risk_by_project_manager: Vec<RiskBreakdownItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightPortfolioSummary {
    pub total_projects: i64,
    pub high_critical_projects: i64,
    pub overdue_milestones: i64,
    pub completed_late_milestones: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInsight {
    pub id: i64,
    #[serde(rename = "type")]
    pub insight_type: String,
    pub title: String,
    pub description: String,
    pub severity: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightRiskDriver {
    pub driver: String,
    pub projects: i64,
    pub max_risk_score: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightAttentionProject {
    pub id: i64,
    pub mcr_id: String,
    pub project_name: String,
    pub risk_score: i64,
    pub risk_level: String,
    pub reason: String,
    pub next_milestone: String,
    pub next_milestone_date: Option<String>,
    pub owner: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightMilestoneAnalysis {
    pub milestone: String,
    pub overdue: i64,
    pub completed_late: i64,
    pub completed_on_time: i64,
    pub average_delay_days: f64,
}
