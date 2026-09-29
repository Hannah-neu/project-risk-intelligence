import { Check, Cpu, Minus } from "lucide-react";
import type { ProjectDetail } from "../../types/projects";
import { displayValue } from "../../utils/projectFormatters";
import { DetailCardHeader } from "./ProjectOverview";

export function ComponentScope({ project }: { project: ProjectDetail }) {
  const scopes = [
    ["Injector", project.hasInjector], ["Pump", project.hasPump], ["Rail", project.hasRail],
    ["ECU", project.hasEcu], ["DNOX", project.hasDnox]
  ] as const;
  return (
    <section className="card detail-card components-card">
      <DetailCardHeader icon={<Cpu size={18} />} title="Component Scope" subtitle="Product scope and newly released content" />
      <div className="scope-copy">
        <div><span>Components</span><strong>{displayValue(project.components)}</strong></div>
        <div><span>New released components</span><strong>{displayValue(project.newReleasedComponents)}</strong></div>
      </div>
      <div className="component-chips">
        {scopes.map(([name, active]) => <span className={active ? "active" : "inactive"} key={name}>{active ? <Check size={13} /> : <Minus size={13} />}{name}</span>)}
      </div>
      <div className="other-component"><span>Other Component</span><strong>{displayValue(project.otherComponent)}</strong></div>
    </section>
  );
}
