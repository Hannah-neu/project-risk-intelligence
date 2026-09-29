import { Cell, Pie, PieChart, ResponsiveContainer, Tooltip } from "recharts";
import type { RiskOverviewData } from "../../types/risks";

const colors = { Critical: "#dc2638", High: "#f2762e", Medium: "#e8aa22", Low: "#3b82f6" };

export function RiskDistributionChart({ data }: { data: RiskOverviewData }) {
  const distribution = [
    { name: "Critical", value: data.critical }, { name: "High", value: data.high },
    { name: "Medium", value: data.medium }, { name: "Low", value: data.low }
  ];
  return (
    <section className="card risk-distribution-card">
      <div className="risk-section-header"><div><span>Current state</span><h2>Risk Distribution</h2><p>Project count by calculated risk level</p></div><strong>{data.totalProjects}<small>projects</small></strong></div>
      <div className="risk-donut-body">
        <div className="risk-donut">
          <ResponsiveContainer width="100%" height="100%">
            <PieChart><Pie data={distribution} dataKey="value" nameKey="name" innerRadius={62} outerRadius={88} paddingAngle={2} stroke="#fff" strokeWidth={3}>{distribution.map((item) => <Cell key={item.name} fill={colors[item.name as keyof typeof colors]} />)}</Pie><Tooltip contentStyle={{ border: "1px solid #e6eaf0", borderRadius: 9, fontSize: 12 }} /></PieChart>
          </ResponsiveContainer>
          <div><strong>{data.critical + data.high}</strong><span>High attention</span></div>
        </div>
        <div className="risk-donut-legend">{distribution.map((item) => <div key={item.name}><i style={{ background: colors[item.name as keyof typeof colors] }} /><span>{item.name}</span><strong>{item.value}</strong></div>)}</div>
      </div>
    </section>
  );
}
