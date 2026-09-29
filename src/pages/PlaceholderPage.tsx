import { ArrowLeft, Bot, FolderKanban, Settings2, ShieldAlert } from "lucide-react";
import { Link } from "react-router-dom";
import { PageHeader } from "../components/common/PageHeader";

type PlaceholderType = "projects" | "project-detail" | "risks" | "ai-insights" | "settings";

const content: Record<PlaceholderType, { title: string; description: string; label: string; icon: typeof FolderKanban }> = {
  projects: { title: "Projects", description: "Browse and analyze all active projects.", label: "Project search, filtering and pagination arrive with the data layer.", icon: FolderKanban },
  "project-detail": { title: "Project Detail", description: "A focused view of project scope, milestones and risk.", label: "This page will be connected to SQLite project records in a later phase.", icon: FolderKanban },
  risks: { title: "Project Risk Overview", description: "Identify and prioritize projects requiring attention.", label: "Risk distribution and current-state analysis will use the deterministic risk engine.", icon: ShieldAlert },
  "ai-insights": { title: "AI Insights", description: "Explore rule-based portfolio insights and attention areas.", label: "Dashboard summaries are generated locally from the current SQLite dataset. The full analysis workspace is planned for Step 10.", icon: Bot },
  settings: { title: "Settings", description: "Review risk rules, AI configuration and data preferences.", label: "Settings are display-only until application persistence is available.", icon: Settings2 }
};

export function PlaceholderPage({ type }: { type: PlaceholderType }) {
  const page = content[type];
  const Icon = page.icon;
  return (
    <div className="page placeholder-page">
      <PageHeader title={page.title} description={page.description} />
      <section className="card placeholder-card">
        <div className="placeholder-icon"><Icon size={30} /></div>
        <span>Stage 1 foundation</span>
        <h2>{page.title} is ready for the next data phase.</h2>
        <p>{page.label}</p>
        <Link to="/" className="secondary-button"><ArrowLeft size={16} /> Back to overview</Link>
      </section>
    </div>
  );
}
