import { CircleCheckBig, ClockAlert, FolderKanban, ShieldAlert } from "lucide-react";
import type { DashboardStat } from "../../types/dashboard";

const icons = {
  projects: FolderKanban,
  onTrack: CircleCheckBig,
  critical: ShieldAlert,
  overdue: ClockAlert
};

export function KpiCard({ stat }: { stat: DashboardStat }) {
  const Icon = icons[stat.icon];
  return (
    <article className={`kpi-card tone-${stat.tone}`}>
      <div className="kpi-icon"><Icon size={23} strokeWidth={1.8} /></div>
      <div className="kpi-copy">
        <span>{stat.label}</span>
        <strong>{stat.value}</strong>
        <small>{stat.detail}</small>
      </div>
      <div className="kpi-accent" />
    </article>
  );
}
