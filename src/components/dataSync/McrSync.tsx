import { useMutation, useQueryClient } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { AlertCircle, CalendarClock, CheckCircle2, ChevronDown, RefreshCw, TriangleAlert, Upload } from "lucide-react";
import { useState } from "react";
import { createPortal } from "react-dom";
import { ImportIssuesTable } from "./ImportIssuesTable";
import { PreviewMetric } from "./PreviewMetric";
import { applyMcrPreview, cancelMcrPreview, previewMcrExcel } from "../../services/database/databaseClient";
import type { ImportFailure, ImportIssueSeverity, McrApplyResult, McrPreview } from "../../types/database";

export function McrSync() {
  const client = useQueryClient();
  const [preview, setPreview] = useState<McrPreview | null>(null);
  const [result, setResult] = useState<McrApplyResult | null>(null);
  const [failure, setFailure] = useState<ImportFailure | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [showIssues, setShowIssues] = useState(false);
  const [issueFilter, setIssueFilter] = useState<ImportIssueSeverity | null>(null);
  const [showChanges, setShowChanges] = useState(false);
  const previewMutation = useMutation({
    mutationFn: previewMcrExcel,
    onSuccess: (data) => { setPreview(data); setResult(null); setFailure(null); setShowIssues(false); setIssueFilter(null); setShowChanges(false); },
    onError: (error) => { setPreview(null); setFailure(normalizeFailure(error)); }
  });
  const applyMutation = useMutation({
    mutationFn: applyMcrPreview,
    onSuccess: async (data) => {
      setResult(data); setPreview(null); setFailure(null); setConfirming(false);
      await Promise.all(["dashboard", "projects", "project-detail", "risk-overview", "insight-overview", "database-status", "sync-history"].map(
        (key) => client.invalidateQueries({ queryKey: [key] })
      ));
    },
    onError: (error) => { setConfirming(false); setFailure(normalizeFailure(error)); }
  });

  async function selectFile() {
    if (!isTauri()) {
      setFailure({ code: "desktop_required", message: "MCR preview requires the Tauri desktop application.", details: [] });
      return;
    }
    setFailure(null);
    const path = await open({ multiple: false, directory: false, title: "Preview MCR Excel", filters: [{ name: "Excel workbook", extensions: ["xlsx", "xlsm"] }] });
    if (typeof path === "string") previewMutation.mutate(path);
  }

  async function cancelPreview() {
    if (!preview) return;
    try {
      await cancelMcrPreview(preview.previewToken);
      setPreview(null); setConfirming(false); setFailure(null); setShowIssues(false); setIssueFilter(null); setShowChanges(false);
    } catch (error) { setFailure(normalizeFailure(error)); }
  }

  const busy = previewMutation.isPending || applyMutation.isPending;
  const changedItems = preview?.items.filter((item) => item.changed) ?? [];
  const changedDates = changedItems.length;
  const actionableIssues = preview?.issues.filter((issue) => issue.severity !== "info") ?? [];
  const visibleIssues = issueFilter ? actionableIssues.filter((issue) => issue.severity === issueFilter) : actionableIssues;

  function selectIssueFilter(severity: ImportIssueSeverity) {
    setShowIssues(true);
    setIssueFilter((current) => current === severity ? null : severity);
  }

  return <section className="mcr-sync-section" aria-labelledby="mcr-sync-title">
    <div className="mcr-sync-hero card">
      <span className="mcr-sync-icon"><CalendarClock size={23} /></span>
      <div className="mcr-sync-copy">
        <span className="eyebrow">Additional source · targeted update</span>
        <h2 id="mcr-sync-title">MCR milestone plan dates</h2>
        <p>Review milestone plan-date updates before applying them to existing projects.</p>
      </div>
      <button className="primary-button" onClick={selectFile} disabled={busy}>
        {busy ? <RefreshCw className="spin" size={17} /> : <Upload size={17} />}
        {previewMutation.isPending ? "Analyzing MCR…" : "Preview MCR Excel"}
      </button>
    </div>

    {failure && <div className="sync-message error" role="alert"><AlertCircle size={20} /><div><strong>MCR sync failed</strong><p>{failure.message}</p>{failure.details.length > 0 && <ul>{failure.details.map((detail) => <li key={detail}>{detail}</li>)}</ul>}</div></div>}
    {result && <div className="import-success card" role="status"><span><CheckCircle2 size={24} /></span><div><strong>MCR updates applied</strong><p>{result.updatedMilestones} planned dates updated across {result.projectsAffected} projects.</p><small>Source: {result.fileName} · Actual dates were not changed.</small></div></div>}

    {preview && <div className="mcr-preview card">
      <div className="preview-heading">
        <div><h2>MCR Import Preview</h2><p className="preview-file">File: <strong>{preview.fileName}</strong></p>
          {preview.errorCount > 0 && <p className="preview-blocker"><TriangleAlert size={15} />Correct the source Excel before applying updates.</p>}</div>
        <div className="preview-gate"><div>
          <button className="cancel-preview-button" onClick={cancelPreview} disabled={applyMutation.isPending}>Cancel</button>
          <button className="replace-button" onClick={() => setConfirming(true)} disabled={preview.errorCount > 0 || busy}>Apply Updates</button>
        </div></div>
      </div>
      <div className="preview-metrics compact mcr">
        <PreviewMetric label="Matched projects" value={preview.matchedProjects} />
        <PreviewMetric label="Milestones to update" value={changedDates} />
        <PreviewMetric label="Warnings" value={preview.warningCount} tone="warning" onSelect={() => selectIssueFilter("warning")} active={showIssues && issueFilter === "warning"} controls="mcr-issues" />
        <PreviewMetric label="Errors" value={preview.errorCount} tone="error" onSelect={() => selectIssueFilter("error")} active={showIssues && issueFilter === "error"} controls="mcr-issues" />
      </div>
      {(changedDates > 0 || actionableIssues.length > 0) && <div className="preview-detail-actions">
        {changedDates > 0 && <button className="preview-detail-toggle" onClick={() => setShowChanges((open) => !open)} aria-expanded={showChanges} aria-controls="mcr-changes">
          <ChevronDown size={16} className={showChanges ? "open" : ""} />{showChanges ? "Hide Changes" : "View Changes"}
        </button>}
        {actionableIssues.length > 0 && <button className="preview-detail-toggle" onClick={() => { setShowIssues((open) => !open); setIssueFilter(null); }} aria-expanded={showIssues} aria-controls="mcr-issues">
          <ChevronDown size={16} className={showIssues ? "open" : ""} />{showIssues ? "Hide Issues" : "View Issues"}
        </button>}
      </div>}
      {showChanges && changedDates > 0 && <div className="mcr-table-wrap preview-detail-panel" id="mcr-changes"><table className="mcr-table"><thead><tr><th>PRJ MCR-ID</th><th>Project</th><th>Stage</th><th>Current plan</th><th>MCR latest date</th><th>Change</th></tr></thead><tbody>
        {changedItems.map((item) => <tr key={`${item.projectId}-${item.milestoneType}`}>
          <td><strong>{item.mcrId}</strong></td><td>{item.projectName}</td><td>{item.milestoneType}</td>
          <td>{item.currentPlanDate ?? "—"}</td><td>{item.newPlanDate}</td>
          <td><span className={`mcr-change ${item.changed ? "changed" : "same"}`}>{item.changed ? item.currentPlanDate === item.newPlanDate ? "Source → MCR" : `${item.currentPlanDate ?? "—"} → ${item.newPlanDate}` : "No change"}</span></td>
        </tr>)}</tbody></table></div>}
      {showIssues && actionableIssues.length > 0 && <ImportIssuesTable id="mcr-issues" issues={visibleIssues} />}
    </div>}

    {confirming && preview && createPortal(<div className="confirm-backdrop" role="presentation"><section className="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="mcr-confirm-title">
      <span className="confirm-icon"><CalendarClock size={22} /></span><h2 id="mcr-confirm-title">Apply MCR plan-date updates?</h2>
      <p>{changedDates} milestone planned dates or sources will be updated across {preview.projectsAffected} projects from:</p><strong>{preview.fileName}</strong>
      <p>Project records and actual milestone dates will not be replaced. Risk scores and insights will be recalculated.</p>
      <div className="confirm-actions"><button onClick={() => setConfirming(false)}>Cancel</button><button className="danger" onClick={() => applyMutation.mutate(preview.previewToken)} disabled={busy}>Apply Updates</button></div>
    </section></div>, document.body)}
  </section>;
}

function normalizeFailure(error: unknown): ImportFailure {
  if (typeof error === "object" && error !== null && "message" in error) {
    const value = error as Partial<ImportFailure>;
    return { code: value.code ?? "mcr_failed", message: String(value.message), details: Array.isArray(value.details) ? value.details.map(String) : [] };
  }
  return { code: "mcr_failed", message: String(error), details: [] };
}
