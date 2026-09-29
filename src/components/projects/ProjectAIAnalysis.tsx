import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { LoaderCircle, Sparkles } from "lucide-react";
import { Link } from "react-router-dom";
import { enterpriseErrorMessage } from "../../services/ai/enterpriseClient";
import { generateProjectAIAnalysis, loadProjectAIAnalysis } from "../../services/database/databaseClient";
import type { AnalysisLanguage } from "../../types/projectAnalysis";
import { formatProjectDateTime } from "../../utils/projectFormatters";
import { useRef, useState } from "react";
import "./ProjectAIAnalysis.css";

export function ProjectAIAnalysis({ projectId }: { projectId: number }) {
  const client = useQueryClient();
  const requestInFlight = useRef(false);
  const [language, setLanguage] = useState<AnalysisLanguage>(() => typeof navigator !== "undefined" && navigator.language.toLowerCase().startsWith("zh") ? "zh" : "en");
  const saved = useQuery({
    queryKey: ["project-ai-analysis", projectId],
    queryFn: () => loadProjectAIAnalysis(projectId)
  });
  const generate = useMutation({
    mutationFn: () => generateProjectAIAnalysis(projectId, language),
    onSuccess: (report) => client.setQueryData(["project-ai-analysis", projectId], report),
    onSettled: () => { requestInFlight.current = false; }
  });
  const report = saved.data;
  const pending = generate.isPending;
  const notConfigured = generate.error && typeof generate.error === "object" && "code" in generate.error && generate.error.code === "not_configured";

  return <section className="card detail-card project-ai-analysis-card" aria-label="AI Project Analysis">
    <div className="project-ai-analysis-header">
      <span className="project-ai-analysis-icon"><Sparkles size={19} /></span>
      <div className="project-ai-analysis-heading"><h2>AI Project Analysis</h2><p>On-demand, single-project report based on current SQLite data</p></div>
      <div className="project-ai-analysis-actions">
        <label>Language <select value={language} onChange={(event) => setLanguage(event.target.value as AnalysisLanguage)} disabled={pending}><option value="en">English</option><option value="zh">中文</option></select></label>
        <button type="button" onClick={() => { if (requestInFlight.current) return; requestInFlight.current = true; generate.mutate(); }} disabled={pending || saved.isLoading}>
          {pending ? <LoaderCircle className="spin" size={16} /> : <Sparkles size={16} />}
          {pending ? "Analyzing project..." : report ? "Regenerate Analysis" : "Generate AI Analysis"}
        </button>
      </div>
    </div>
    {saved.isLoading && <p className="project-ai-analysis-note">Checking for a saved analysis…</p>}
    {saved.isError && <p className="project-ai-analysis-error">Could not load the saved analysis. Please reopen this project.</p>}
    {generate.isError && <div className="project-ai-analysis-error" role="alert">
      {enterpriseErrorMessage(generate.error)}
      {notConfigured && <> <Link to="/settings">Open AI Configuration</Link></>}
    </div>}
    {!saved.isLoading && !report && !pending && <p className="project-ai-analysis-note">No report has been generated. Click the button to analyze this project; opening the page never calls the LLM.</p>}
    {pending && <p className="project-ai-analysis-progress" role="status"><LoaderCircle className="spin" size={17} /> Analyzing project...</p>}
    {report && <div className="project-ai-analysis-body">
      {report.outdated && <div className="project-ai-analysis-stale"><strong>Analysis outdated</strong><span>This analysis was generated using an older project dataset. Regenerate it for the current data.</span></div>}
      <div className="project-ai-analysis-meta"><span>Generated {formatProjectDateTime(report.createdAt)}</span><span>Model {report.model}</span><span>Language {report.language === "zh" ? "中文" : "English"}</span></div>
      <div className="project-ai-analysis-content">{report.content}</div>
    </div>}
  </section>;
}
