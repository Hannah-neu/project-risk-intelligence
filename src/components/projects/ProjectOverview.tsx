import { BriefcaseBusiness } from "lucide-react";
import type { ProjectDetail } from "../../types/projects";
import { displayValue } from "../../utils/projectFormatters";

export function ProjectOverview({ project }: { project: ProjectDetail }) {
  const fields = [
    ["No", project.projectNo], ["PRJ MCR-ID", project.mcrId], ["PRJ name", project.projectName],
    ["Customer", project.customer], ["P-ID", project.pId], ["Acq manager", project.acqManager],
    ["Project manager", project.projectManager], ["Engine Segment", project.engineSegment],
    ["Prod Category", project.productCategory], ["Emission", project.emission], ["(A)PIC", project.apic],
    ["A/K Project", project.projectType], ["System", project.system]
  ] as const;

  return (
    <section className="card detail-card overview-card">
      <DetailCardHeader icon={<BriefcaseBusiness size={18} />} title="Project Overview" subtitle="Core ownership and classification from the imported workbook" />
      <dl className="detail-field-grid">
        {fields.map(([label, value]) => <div key={label}><dt>{label}</dt><dd>{displayValue(value)}</dd></div>)}
      </dl>
      <div className="source-strip"><span>Source</span><strong>{project.sourceFile}</strong><small>Excel row {project.sourceRow}</small></div>
    </section>
  );
}

export function DetailCardHeader({ icon, title, subtitle }: { icon: React.ReactNode; title: string; subtitle: string }) {
  return <div className="detail-card-header"><span>{icon}</span><div><h2>{title}</h2><p>{subtitle}</p></div></div>;
}
