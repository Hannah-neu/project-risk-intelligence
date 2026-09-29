import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { lazy, Suspense } from "react";
import { KpiCard } from "../components/dashboard/KpiCard";
import { PageHeader } from "../components/common/PageHeader";
import { getDashboardData, getRiskOverview } from "../services/database/databaseClient";
import type { DashboardStat } from "../types/dashboard";
import "./DashboardRisk.css";

const desktopMode = isTauri();
const DashboardRiskPanels = lazy(() => import("./DashboardRiskPanels").then((module) => ({ default: module.DashboardRiskPanels })));

export function Dashboard() {
  const dashboardQuery = useQuery({
    queryKey: ["dashboard"],
    queryFn: getDashboardData,
    enabled: desktopMode
  });
  const riskQuery = useQuery({
    queryKey: ["risk-overview", "dashboard"],
    queryFn: () => getRiskOverview({ limit: 1, offset: 0 }),
    enabled: desktopMode
  });
  const data = dashboardQuery.data;
  const totalProjects = data?.totalProjects ?? 0;
  const stats: DashboardStat[] = [
    { label: "Total Projects", value: totalProjects, detail: data?.sourceFile ?? "Current dataset", tone: "primary", icon: "projects" },
    { label: "On Track", value: data?.onTrackProjects ?? 0, detail: percentage(data?.onTrackProjects ?? 0, totalProjects), tone: "success", icon: "onTrack" },
    { label: "Critical", value: data?.criticalProjects ?? 0, detail: percentage(data?.criticalProjects ?? 0, totalProjects), tone: "danger", icon: "critical" },
    { label: "Overdue", value: data?.overdueProjects ?? 0, detail: percentage(data?.overdueProjects ?? 0, totalProjects), tone: "warning", icon: "overdue" }
  ];

  return (
    <div className="page dashboard-page">
      <PageHeader
        eyebrow="Portfolio Command Center"
        title="Project Portfolio Overview"
        description="Monitor portfolio health and explore current risk concentration."
        aside={desktopMode ? `${data?.sourceFile ?? "Local dataset"} · ${totalProjects} projects` : "Desktop database unavailable"}
      />

      {!desktopMode && <section className="sync-message error" role="status">Open the Tauri desktop application to load the local portfolio database.</section>}
      {dashboardQuery.isError && <section className="sync-message error" role="alert">Dashboard data could not be loaded from the local database.</section>}

      <section className="kpi-grid risk-kpi-grid" aria-label="Portfolio status statistics">
        {stats.map((stat) => <KpiCard key={stat.label} stat={stat} />)}
      </section>

      {riskQuery.isError && <section className="sync-message error" role="alert">Risk distribution and analysis could not be loaded from the local database.</section>}
      {riskQuery.data && <Suspense fallback={<RiskPanelsLoading />}><DashboardRiskPanels data={riskQuery.data} /></Suspense>}
      {desktopMode && riskQuery.isLoading && <RiskPanelsLoading />}
    </div>
  );
}

function RiskPanelsLoading() {
  return <div className="dashboard-risk-grid dashboard-risk-loading" aria-label="Loading portfolio risk charts"><div className="card" /><div className="card" /></div>;
}

function percentage(value: number, total: number): string {
  return total === 0 ? "0% of total" : `${Math.round((value / total) * 100)}% of total`;
}
