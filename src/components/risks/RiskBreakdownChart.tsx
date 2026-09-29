import { useState } from "react";
import { Bar, BarChart, CartesianGrid, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import type { RiskBreakdowns } from "../../types/risks";

type Dimension = keyof RiskBreakdowns;
const dimensions: { key: Dimension; label: string }[] = [
  { key: "customers", label: "Customer" }, { key: "projectManagers", label: "Project Manager" },
  { key: "systems", label: "System" }, { key: "productCategories", label: "Product Category" }
];

export function RiskBreakdownChart({ breakdowns }: { breakdowns: RiskBreakdowns }) {
  const [dimension, setDimension] = useState<Dimension>("customers");
  const data = breakdowns[dimension];
  return (
    <section className="card risk-breakdown-card">
      <div className="risk-section-header"><div><span>Portfolio concentration</span><h2>Risk Analysis</h2><p>Top groups ranked by cumulative risk exposure</p></div></div>
      <div className="risk-dimension-tabs" role="tablist">{dimensions.map((item) => <button type="button" role="tab" aria-selected={dimension === item.key} className={dimension === item.key ? "active" : ""} onClick={() => setDimension(item.key)} key={item.key}>{item.label}</button>)}</div>
      <div className="risk-bar-chart">
        <ResponsiveContainer width="100%" height="100%">
          <BarChart data={data} layout="vertical" margin={{ top: 4, right: 18, left: 8, bottom: 3 }} barSize={13}>
            <CartesianGrid horizontal={false} stroke="#edf0f4" />
            <XAxis type="number" allowDecimals={false} tick={{ fontSize: 12, fill: "#8490a3" }} axisLine={false} tickLine={false} />
            <YAxis type="category" dataKey="label" width={132} interval={0} tick={{ fontSize: 12, fill: "#5c687c" }} axisLine={false} tickLine={false} tickFormatter={(value: string) => value.length > 16 ? `${value.slice(0, 15)}…` : value} />
            <Tooltip contentStyle={{ border: "1px solid #e6eaf0", borderRadius: 9, fontSize: 12 }} cursor={{ fill: "#f8fafc" }} />
            <Bar dataKey="critical" name="Critical" stackId="risk" fill="#dc2638" radius={[3, 0, 0, 3]} />
            <Bar dataKey="high" name="High" stackId="risk" fill="#f2762e" />
            <Bar dataKey="medium" name="Medium" stackId="risk" fill="#e8aa22" />
            <Bar dataKey="low" name="Low" stackId="risk" fill="#3b82f6" radius={[0, 3, 3, 0]} />
          </BarChart>
        </ResponsiveContainer>
      </div>
      <div className="risk-chart-legend"><span><i className="critical" />Critical</span><span><i className="high" />High</span><span><i className="medium" />Medium</span><span><i className="low" />Low</span></div>
    </section>
  );
}
