import type { RiskLevel } from "../../types/dashboard";

export function RiskBadge({ level }: { level: RiskLevel }) {
  return <span className={`badge risk-${level.toLowerCase()}`}>{level}</span>;
}

export function StatusBadge({ status }: { status: string }) {
  return <span className="badge business-status-badge">{status}</span>;
}
