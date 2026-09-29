import type { ImportIssue } from "../../types/database";

export function ImportIssuesTable({ issues, id }: { issues: ImportIssue[]; id: string }) {
  return (
    <div className="issue-table-wrap preview-detail-panel" id={id}>
      <table className="issue-table">
        <thead><tr><th>Severity</th><th>Row</th><th>Field</th><th>Original Value</th><th>Normalized Value</th><th>Issue</th></tr></thead>
        <tbody>
          {issues.map((issue, index) => (
            <tr key={`${issue.category}-${issue.row}-${issue.field}-${index}`}>
              <td><span className={`severity-badge ${issue.severity}`}>{issue.severity}</span></td>
              <td>{issue.row ?? "—"}</td>
              <td><strong>{issue.field ?? "Workbook"}</strong></td>
              <td className="issue-value">{issue.originalValue ?? "—"}</td>
              <td className="issue-value normalized">{issue.normalizedValue ?? "null"}</td>
              <td>{issue.issue}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
