import { useQuery } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { Bot } from "lucide-react";
import { AskProjectData } from "../components/insights/AskProjectData";
import { PortfolioInsightSummary } from "../components/insights/PortfolioInsightSummary";
import { PageHeader } from "../components/common/PageHeader";
import { getInsightOverview } from "../services/database/databaseClient";
import { formatProjectDateTime } from "../utils/projectFormatters";

const desktopMode = isTauri();

export function AIInsights() {
  const query = useQuery({ queryKey: ["insight-overview"], queryFn: getInsightOverview, enabled: desktopMode });
  const data = query.data;
  return <div className="page ai-insights-page"><PageHeader eyebrow="Local Decision Support" title="AI Insights" description="Current local portfolio summary and on-demand project questions." aside={data ? `Generated ${formatProjectDateTime(data.generatedAt)}` : "Local summary"} />
    {!desktopMode && <InsightState title="Desktop application required" message="Local insights require the Tauri SQLite database." />}
    {desktopMode && query.isLoading && <div className="insights-loading"><span /><span /></div>}
    {desktopMode && query.isError && <InsightState title="Insights unavailable" message="The rule-based portfolio analysis could not be loaded." />}
    {data && <><PortfolioInsightSummary summary={data.summary} lead={data.insights.find((item) => item.type === "summary")} /><AskProjectData /></>}
  </div>;
}

function InsightState({ title, message }: { title: string; message: string }) {
  return <section className="card insight-state"><Bot size={31} /><strong>{title}</strong><p>{message}</p></section>;
}
