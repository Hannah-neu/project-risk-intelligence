import { Bot, CalendarCheck, CalendarClock, Gauge, ShieldAlert } from "lucide-react";
import type { InsightPortfolioSummary, RuleInsight } from "../../types/insights";

export function PortfolioInsightSummary({ summary, lead }: { summary: InsightPortfolioSummary; lead?: RuleInsight }) {
  const metrics = [
    { label: "Projects", value: summary.totalProjects, icon: Gauge },
    { label: "High / Critical", value: summary.highCriticalProjects, icon: ShieldAlert },
    { label: "Overdue milestones", value: summary.overdueMilestones, icon: CalendarClock },
    { label: "Completed late", value: summary.completedLateMilestones, icon: CalendarCheck }
  ];
  return (
    <section className="card insight-summary-hero">
      <div className="insight-summary-copy"><span className="insight-local-mark"><Bot size={15} /> Rule-based · Local</span><h2>{lead?.title ?? "Portfolio analysis is ready"}</h2><p>{lead?.description ?? "Insights are calculated from the current imported dataset."}</p><small>No LLM or historical comparison is used.</small></div>
      <div className="insight-summary-metrics">{metrics.map(({ label, value, icon: Icon }) => <div key={label}><Icon size={16} /><span>{label}</span><strong>{value}</strong></div>)}</div>
    </section>
  );
}
