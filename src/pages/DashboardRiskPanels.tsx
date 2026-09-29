import { RiskBreakdownChart } from "../components/risks/RiskBreakdownChart";
import { RiskDistributionChart } from "../components/risks/RiskDistributionChart";
import type { RiskOverviewData } from "../types/risks";

export function DashboardRiskPanels({ data }: { data: RiskOverviewData }) {
  return <div className="dashboard-risk-grid">
    <RiskDistributionChart data={data} />
    <RiskBreakdownChart breakdowns={data.breakdowns} />
  </div>;
}
