import { CircleAlert, ShieldCheck } from "lucide-react";
import type { ProjectRiskDetail } from "../../types/projects";
import { formatProjectDateTime } from "../../utils/projectFormatters";
import { RiskBadge } from "../common/Badge";
import { DetailCardHeader } from "./ProjectOverview";

export function RiskAnalysis({ risk }: { risk: ProjectRiskDetail }) {
  return (
    <section className="card detail-card risk-analysis-card">
      <DetailCardHeader icon={<CircleAlert size={18} />} title="Risk Analysis" subtitle={`Calculated locally · ${formatProjectDateTime(risk.calculatedAt)}`} />
      <div className="risk-analysis-body">
        <div className="risk-score-block"><span>Risk score</span><strong>{risk.riskScore}</strong><small>out of 100</small><div><RiskBadge level={risk.riskLevel} /></div></div>
        <div className="risk-driver-block">
          <span>Primary risk driver</span>
          <strong>{risk.primaryRiskDriver || "No significant risk driver"}</strong>
          <ul>
            {risk.riskReasons.map((reason) => <li key={reason}><CircleAlert size={14} />{reason}</li>)}
            {risk.riskReasons.length === 0 && <li className="risk-clear"><ShieldCheck size={15} />No active rule-based risk reasons.</li>}
          </ul>
        </div>
      </div>
    </section>
  );
}
