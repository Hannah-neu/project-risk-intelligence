import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { FolderX, LoaderCircle } from "lucide-react";
import { Link, useParams } from "react-router-dom";
import { ComponentScope } from "../components/projects/ComponentScope";
import { MilestoneTimeline } from "../components/projects/MilestoneTimeline";
import { ProjectHeader } from "../components/projects/ProjectHeader";
import { ProjectOverview } from "../components/projects/ProjectOverview";
import { RiskAnalysis } from "../components/projects/RiskAnalysis";
import { SamplesCard } from "../components/projects/SamplesCard";
import { ProjectAIAnalysis } from "../components/projects/ProjectAIAnalysis";
import { getProjectDetail } from "../services/database/databaseClient";

const desktopMode = isTauri();

export function ProjectDetail() {
  const { id } = useParams();
  const projectId = Number(id);
  const validId = Number.isInteger(projectId) && projectId > 0;
  const query = useQuery({
    queryKey: ["project-detail", projectId],
    queryFn: () => getProjectDetail(projectId),
    enabled: desktopMode && validId
  });

  if (!desktopMode) return <DetailState icon={<FolderX size={31} />} title="Desktop application required" message="Project details are stored in the local Tauri SQLite database." />;
  if (!validId) return <DetailState icon={<FolderX size={31} />} title="Invalid project" message="The project identifier in this address is not valid." />;
  if (query.isLoading) return <div className="page project-detail-page"><div className="detail-loading"><LoaderCircle className="spin" size={27} /><strong>Loading project record…</strong><span>Reading milestones and calculated risk from SQLite</span></div></div>;
  if (query.isError || !query.data) return <DetailState icon={<FolderX size={31} />} title="Project not found" message="This project may no longer exist in the current imported dataset." />;

  const project = query.data;
  return (
    <div className="page project-detail-page">
      <ProjectHeader project={project} />
      <div className="project-detail-grid">
        <ProjectOverview project={project} />
        <ComponentScope project={project} />
        <MilestoneTimeline milestones={project.milestones} />
        <SamplesCard project={project} />
        <RiskAnalysis risk={project.risk} />
        <ProjectAIAnalysis projectId={project.id} />
      </div>
    </div>
  );
}

function DetailState({ icon, title, message }: { icon: React.ReactNode; title: string; message: string }) {
  return <div className="page project-detail-page"><section className="card detail-state">{icon}<strong>{title}</strong><p>{message}</p><Link to="/projects" className="secondary-button">Back to projects</Link></section></div>;
}
