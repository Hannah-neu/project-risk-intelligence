import { Boxes } from "lucide-react";
import type { ProjectDetail } from "../../types/projects";
import { formatProjectDate } from "../../utils/projectFormatters";
import { DetailCardHeader } from "./ProjectOverview";

export function SamplesCard({ project }: { project: ProjectDetail }) {
  const samples = [["B", project.bSample], ["C", project.cSample], ["D", project.dSample]] as const;
  return (
    <section className="card detail-card samples-card">
      <DetailCardHeader icon={<Boxes size={18} />} title="Samples" subtitle="Key prototype and validation sample dates" />
      <div className="sample-grid">
        {samples.map(([name, date]) => <div key={name}><span>{name}</span><div><strong>{name} Sample</strong><small>{formatProjectDate(date)}</small></div></div>)}
      </div>
    </section>
  );
}
