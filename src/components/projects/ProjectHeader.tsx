import { ArrowLeft, CalendarClock, Database, UserRound } from "lucide-react";
import { Link } from "react-router-dom";
import type { ProjectDetail } from "../../types/projects";
import { formatProjectDateTime } from "../../utils/projectFormatters";
import { RiskBadge, StatusBadge } from "../common/Badge";

export function ProjectHeader({ project }: { project: ProjectDetail }) {
  return (
    <header className="project-detail-header card">
      <div className="detail-header-main">
        <Link to="/projects" className="detail-back" aria-label="Back to projects"><ArrowLeft size={17} /></Link>
        <div className="detail-identity">
          <span className="eyebrow">Project record · {project.mcrId}</span>
          <h1>{project.projectName}</h1>
          <div className="detail-meta">
            <span><Database size={14} />{project.customer || "Customer not assigned"}</span>
            <span><UserRound size={14} />{project.projectManager || "Manager not assigned"}</span>
            <span><CalendarClock size={14} />Updated {formatProjectDateTime(project.updatedAt)}</span>
          </div>
        </div>
      </div>
      <div className="detail-health-panel">
        <div className={`risk-orbit risk-orbit-${project.risk.riskLevel.toLowerCase()}`}>
          <strong>{project.risk.riskScore}</strong><span>/ 100</span>
        </div>
        <div className="detail-health-copy">
          <span>Calculated risk</span>
          <div><RiskBadge level={project.risk.riskLevel} /></div>
          <small>PRJ Status: <StatusBadge status={project.projectStatus || "Unspecified"} /></small>
        </div>
      </div>
    </header>
  );
}
