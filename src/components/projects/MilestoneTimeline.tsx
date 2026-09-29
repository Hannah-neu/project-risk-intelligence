import { Flag } from "lucide-react";
import type { ProjectMilestoneDetail } from "../../types/projects";
import { formatProjectDate } from "../../utils/projectFormatters";
import { DetailCardHeader } from "./ProjectOverview";

export function MilestoneTimeline({ milestones }: { milestones: ProjectMilestoneDetail[] }) {
  return (
    <section className="card detail-card milestone-detail-card">
      <DetailCardHeader icon={<Flag size={18} />} title="Milestone Timeline" subtitle="Plan versus actual delivery across the release lifecycle" />
      <div className="milestone-detail-grid">
        {milestones.map((milestone, index) => (
          <article className={`milestone-step ${statusClass(milestone.status)}`} key={milestone.id}>
            <div className="milestone-step-head"><i>{index + 1}</i><div><strong>{milestone.milestoneType}</strong><span>{milestone.status}</span></div></div>
            <dl>
              <div><dt>Plan</dt><dd>{formatProjectDate(milestone.plannedDate)}</dd></div>
              <div><dt>Actual</dt><dd>{formatProjectDate(milestone.actualDate)}</dd></div>
              <div><dt>Delta</dt><dd className={deltaClass(milestone.deltaDays)}>{formatDelta(milestone.deltaDays, milestone.status)}</dd></div>
            </dl>
          </article>
        ))}
      </div>
    </section>
  );
}

function statusClass(status: ProjectMilestoneDetail["status"]): string {
  return status.toLowerCase().replace(/ /g, "-");
}

function deltaClass(delta: number | null): string {
  if (delta === null || delta === 0) return "";
  return delta > 0 ? "delta-late" : "delta-early";
}

function formatDelta(delta: number | null, status: ProjectMilestoneDetail["status"]): string {
  if (delta === null) return "—";
  if (delta === 0) return "On date";
  if (status === "Overdue") return `+${delta} days overdue`;
  return `${delta > 0 ? "+" : ""}${delta} days`;
}
