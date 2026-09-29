import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  ChevronDown,
  CheckCircle2,
  Database,
  Download,
  FileCheck2,
  FileSpreadsheet,
  HardDrive,
  History,
  RefreshCw,
  TriangleAlert,
  Upload
} from "lucide-react";
import { useState } from "react";
import { createPortal } from "react-dom";
import { Link } from "react-router-dom";
import { PageHeader } from "../components/common/PageHeader";
import { ImportIssuesTable } from "../components/dataSync/ImportIssuesTable";
import { McrSync } from "../components/dataSync/McrSync";
import { PreviewMetric } from "../components/dataSync/PreviewMetric";
import {
  cancelImportPreview,
  getDatabaseStatus,
  getImportHistory,
  getSyncHistory,
  exportProjectList,
  previewExcel,
  replaceExcelPreview
} from "../services/database/databaseClient";
import type { ImportFailure, ImportIssueSeverity, ImportPreview, ImportResult } from "../types/database";

const desktopMode = isTauri();

export function DataSync() {
  const queryClient = useQueryClient();
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [failure, setFailure] = useState<ImportFailure | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [showIssues, setShowIssues] = useState(false);
  const [issueFilter, setIssueFilter] = useState<ImportIssueSeverity | null>(null);
  const [exportResult, setExportResult] = useState<{ projectCount: number; filePath: string } | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);
  const statusQuery = useQuery({
    queryKey: ["database-status"],
    queryFn: getDatabaseStatus,
    enabled: desktopMode
  });
  const projectHistoryQuery = useQuery({
    queryKey: ["import-history"],
    queryFn: () => getImportHistory(8),
    enabled: desktopMode
  });
  const historyQuery = useQuery({
    queryKey: ["sync-history"],
    queryFn: () => getSyncHistory(8),
    enabled: desktopMode
  });
  const previewMutation = useMutation({
    mutationFn: previewExcel,
    onSuccess: (result) => {
      setFailure(null);
      setResult(null);
      setPreview(result);
      setShowIssues(false);
      setIssueFilter(null);
    },
    onError: (error) => {
      setPreview(null);
      setFailure(normalizeFailure(error));
    }
  });
  const replacementMutation = useMutation({
    mutationFn: replaceExcelPreview,
    onSuccess: async (importResult) => {
      setFailure(null);
      setResult(importResult);
      setPreview(null);
      setConfirming(false);
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["dashboard"] }),
        queryClient.invalidateQueries({ queryKey: ["projects"] }),
        queryClient.invalidateQueries({ queryKey: ["project-detail"] }),
        queryClient.invalidateQueries({ queryKey: ["risk-overview"] }),
        queryClient.invalidateQueries({ queryKey: ["insight-overview"] }),
        queryClient.invalidateQueries({ queryKey: ["database-status"] }),
        queryClient.invalidateQueries({ queryKey: ["import-history"] }),
        queryClient.invalidateQueries({ queryKey: ["sync-history"] })
      ]);
    },
    onError: (error) => {
      setConfirming(false);
      setFailure(normalizeFailure(error));
    }
  });
  const exportMutation = useMutation({
    mutationFn: exportProjectList,
    onSuccess: (result) => {
      setExportError(null);
      setExportResult(result);
    },
    onError: (error) => {
      setExportResult(null);
      setExportError(error instanceof Error ? error.message : String(error));
    }
  });

  const status = statusQuery.data;
  const history = historyQuery.data ?? [];
  const latestSuccessfulImport = projectHistoryQuery.data?.find((item) => item.status === "success");
  const actionableIssues = preview?.issues.filter((issue) => issue.severity !== "info") ?? [];
  const visibleIssues = issueFilter ? actionableIssues.filter((issue) => issue.severity === issueFilter) : actionableIssues;

  function selectIssueFilter(severity: ImportIssueSeverity) {
    setShowIssues(true);
    setIssueFilter((current) => current === severity ? null : severity);
  }

  async function handlePreview() {
    setFailure(null);
    setResult(null);
    if (!desktopMode) {
      setFailure({
        code: "desktop_required",
        message: "Excel preview is available in the Tauri desktop application.",
        details: ["Run npm run tauri dev to use the native file picker and local parser."]
      });
      return;
    }

    const selectedPath = await open({
      multiple: false,
      directory: false,
      title: "Preview Project Excel",
      filters: [{ name: "Excel workbook", extensions: ["xlsx", "xlsm"] }]
    });
    if (typeof selectedPath === "string") previewMutation.mutate(selectedPath);
  }

  async function handleCancelPreview() {
    if (!preview) return;
    try {
      await cancelImportPreview(preview.previewToken);
      setPreview(null);
      setShowIssues(false);
      setIssueFilter(null);
      setConfirming(false);
      setFailure(null);
    } catch (error) {
      setFailure(normalizeFailure(error));
    }
  }

  function handleConfirmedReplacement() {
    if (!preview || preview.errorCount > 0) return;
    replacementMutation.mutate(preview.previewToken);
  }

  async function handleExport() {
    setExportError(null);
    setExportResult(null);
    if (!desktopMode) {
      setExportError("Project List export is available in the Tauri desktop application.");
      return;
    }
    try {
      const selectedPath = await save({
        title: "Export Current Project List",
        defaultPath: `Active_Project_List_Export_${localDateStamp()}.xlsx`,
        filters: [{ name: "Excel workbook", extensions: ["xlsx"] }]
      });
      if (typeof selectedPath === "string") exportMutation.mutate(selectedPath);
    } catch (error) {
      setExportError(error instanceof Error ? error.message : String(error));
    }
  }

  const loading = previewMutation.isPending || replacementMutation.isPending;

  return (
    <div className="page data-sync-page">
      <PageHeader
        eyebrow="Import Assurance"
        title="Data Sync"
        description="Preview your Project List or MCR file before any database change."
        aside={desktopMode ? "Local validation engine ready" : "Browser preview mode"}
      />

      <section className="sync-hero card">
        <div className="sync-hero-copy">
          <span className="sync-icon"><FileSpreadsheet size={25} /></span>
          <div>
            <span className="eyebrow">Safe Excel staging</span>
            <h2>Preview the portfolio before replacement</h2>
            <p>Review project counts and any issues before replacing the current Project List.</p>
          </div>
        </div>
        <button className="primary-button" onClick={handlePreview} disabled={loading}>
          {loading ? <RefreshCw className="spin" size={17} /> : <Upload size={17} />}
          {previewMutation.isPending ? "Analyzing workbook…" : "Import New Excel"}
        </button>
      </section>

      {result && (
        <section className="import-success card" role="status">
          <span><CheckCircle2 size={24} /></span>
          <div>
            <strong>Import completed successfully</strong>
            <p>{result.totalProjects} projects imported · {result.totalMilestones} milestones generated · {result.highRiskProjects} high-risk projects identified</p>
            <small>{result.warningCount} warnings normalized · Source: {result.fileName}</small>
          </div>
          <Link to="/">View Dashboard</Link>
        </section>
      )}

      {failure && (
        <section className="sync-message error" role="alert">
          <AlertCircle size={21} />
          <div>
            <strong>{failure.code === "replacement_failed" ? "Import failed" : "Preview failed"}</strong>
            <p>{failure.message}</p>
            {failure.details.length > 0 && <ul>{failure.details.slice(0, 8).map((detail) => <li key={detail}>{detail}</li>)}</ul>}
          </div>
        </section>
      )}

      {exportResult && <section className="sync-message success" role="status"><CheckCircle2 size={21} /><div><strong>Export completed</strong><p>{exportResult.projectCount} projects exported successfully.</p></div></section>}
      {exportError && <section className="sync-message error" role="alert"><AlertCircle size={21} /><div><strong>Export failed</strong><p>{exportError}</p></div></section>}

      {preview && (
        <section className="preview-workbench card" aria-label="Excel import preview">
          <div className="preview-heading">
            <div>
              <h2>Project List Import Preview</h2>
              <p className="preview-file">File: <strong>{preview.fileName}</strong></p>
              {preview.errorCount > 0 && <p className="preview-blocker"><TriangleAlert size={15} />Correct the source Excel before replacing the dataset.</p>}
            </div>
            <div className="preview-gate">
              <div>
                <button className="cancel-preview-button" onClick={handleCancelPreview} disabled={replacementMutation.isPending}>Cancel</button>
                <button className="replace-button" onClick={() => setConfirming(true)} disabled={preview.errorCount > 0 || replacementMutation.isPending}>
                  <FileCheck2 size={16} />Replace Current Dataset
                </button>
              </div>
            </div>
          </div>

          <div className="preview-metrics compact project-list">
            <PreviewMetric label="Projects Detected" value={preview.projectsDetected} tone="neutral" />
            <PreviewMetric label="Warnings" value={preview.warningCount} tone="warning" onSelect={() => selectIssueFilter("warning")} active={showIssues && issueFilter === "warning"} controls="project-list-issues" />
            <PreviewMetric label="Errors" value={preview.errorCount} tone="error" onSelect={() => selectIssueFilter("error")} active={showIssues && issueFilter === "error"} controls="project-list-issues" />
          </div>

          {actionableIssues.length > 0 && <div className="preview-detail-actions">
            <button className="preview-detail-toggle" onClick={() => { setShowIssues((open) => !open); setIssueFilter(null); }} aria-expanded={showIssues} aria-controls="project-list-issues">
              <ChevronDown size={16} className={showIssues ? "open" : ""} />{showIssues ? "Hide Issues" : "View Issues"}
            </button>
          </div>}
          {showIssues && actionableIssues.length > 0 && <ImportIssuesTable id="project-list-issues" issues={visibleIssues} />}
        </section>
      )}

      {replacementMutation.isPending && (
        <section className="replacement-progress card" aria-live="polite">
          <RefreshCw className="spin" size={22} />
          <div><strong>Replacing dataset…</strong><p>Importing projects · Building milestones · Calculating risks · Generating insights · Finalizing</p></div>
        </section>
      )}

      <McrSync />

      <div className="sync-grid">
        <section className="card dataset-card">
          <div className="card-header compact">
            <div className="title-with-icon"><span className="section-icon"><Database size={18} /></span><h2>Current Dataset</h2></div>
          </div>
          <dl className="dataset-metrics">
            <Metric label="Source file" value={latestSuccessfulImport?.fileName ?? "No dataset imported"} />
            <Metric label="Last import" value={latestSuccessfulImport ? formatDateTime(latestSuccessfulImport.importTime) : "—"} />
            <Metric label="Total projects" value={formatNumber(status?.totalProjects)} />
            <Metric label="Total milestones" value={formatNumber(status?.totalMilestones)} />
            <Metric label="Warnings" value={latestSuccessfulImport ? formatNumber(latestSuccessfulImport.warningCount) : "—"} />
            <Metric label="Database size" value={formatBytes(status?.databaseSizeBytes)} />
            <Metric label="Schema version" value={status ? `Version ${status.schemaVersion}` : "—"} />
          </dl>
          <div className="dataset-export-actions"><button className="primary-button" onClick={handleExport} disabled={exportMutation.isPending || status?.totalProjects === 0}>{exportMutation.isPending ? <RefreshCw className="spin" size={17} /> : <Download size={17} />}{exportMutation.isPending ? "Generating Excel…" : "Export Project List"}</button></div>
          <div className="replacement-note"><HardDrive size={16} /><span>Only a validated and explicitly confirmed preview can replace this complete dataset.</span></div>
        </section>

        <section className="card history-card">
          <div className="card-header compact">
            <div className="title-with-icon"><span className="section-icon"><History size={18} /></span><h2>Import History</h2></div>
          </div>
          {history.length > 0 ? (
            <div className="history-list">
              {history.map((item) => (
                <article key={`${item.kind}-${item.id}`} className="history-item">
                  <span className={`history-status ${item.status}`}><i />{item.status === "success" ? "Completed" : "Failed"}</span>
                  <div><strong>{item.fileName}</strong><p><span className={`history-kind ${item.kind}`}>{item.kind === "mcr" ? "MCR Update" : "Project List"}</span>{formatDateTime(item.eventTime)}</p></div>
                  <span className="history-count">{item.status !== "success" ? "Dataset unchanged" : item.kind === "mcr" ? `${item.updatedMilestones ?? 0} milestones · ${item.projectsAffected ?? 0} projects` : `${item.projectCount ?? 0} projects`}</span>
                </article>
              ))}
            </div>
          ) : (
            <div className="sync-empty"><History size={24} /><strong>No import history</strong><p>Project List imports and MCR updates will appear here.</p></div>
          )}
        </section>
      </div>

      {confirming && preview && createPortal(
        <div className="confirm-backdrop" role="presentation">
          <section className="confirm-dialog" role="dialog" aria-modal="true" aria-labelledby="replace-dialog-title">
            <span className="confirm-icon"><TriangleAlert size={22} /></span>
            <div>
              <h2 id="replace-dialog-title">Replace current dataset?</h2>
              <p>The current project dataset will be completely replaced with the validated data from:</p>
              <strong>{preview.fileName}</strong>
              <p>This action will rebuild project, milestone, risk, and insight data.</p>
            </div>
            <div className="confirm-actions">
              <button onClick={() => setConfirming(false)}>Cancel</button>
              <button className="danger" onClick={handleConfirmedReplacement}>Replace Dataset</button>
            </div>
          </section>
        </div>, document.body
      )}
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div><dt>{label}</dt><dd>{value}</dd></div>;
}

function normalizeFailure(error: unknown): ImportFailure {
  if (typeof error === "object" && error !== null && "message" in error) {
    const candidate = error as Partial<ImportFailure>;
    return {
      code: candidate.code ?? "preview_failed",
      message: typeof candidate.message === "string" ? candidate.message : "The workbook could not be previewed.",
      details: Array.isArray(candidate.details) ? candidate.details.map(String) : []
    };
  }
  return { code: "preview_failed", message: String(error), details: [] };
}

function formatNumber(value: number | undefined): string {
  return value === undefined ? "—" : new Intl.NumberFormat().format(value);
}

function formatDateTime(value: string): string {
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value));
}

function formatBytes(value: number | undefined): string {
  if (value === undefined) return "—";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(1)} MB`;
}

function localDateStamp(): string {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}
