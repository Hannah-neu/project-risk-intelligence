type MetricTone = "neutral" | "warning" | "error";

interface PreviewMetricProps {
  label: string;
  value: number;
  tone?: MetricTone;
  onSelect?: () => void;
  active?: boolean;
  controls?: string;
}

export function PreviewMetric({ label, value, tone = "neutral", onSelect, active = false, controls }: PreviewMetricProps) {
  const content = <><span>{label}</span><strong>{value}</strong></>;
  const className = `preview-metric ${tone}${onSelect ? " selectable" : ""}${active ? " selected" : ""}`;

  if (!onSelect) return <div className={className}>{content}</div>;

  return <button type="button" className={className} onClick={onSelect} disabled={value === 0}
    aria-pressed={active} aria-controls={controls} title={value > 0 ? `Show ${label.toLowerCase()} only` : `No ${label.toLowerCase()}`}>
    {content}
  </button>;
}
