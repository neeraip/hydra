interface MetricChipProps {
  value: string | number;
  label: string;
  /** Override colour of the value text (defaults to var(--text-primary)) */
  valueColor?: string;
  /** Override border colour of the chip */
  borderColor?: string;
}

export function MetricChip({
  value,
  label,
  valueColor,
  borderColor,
}: MetricChipProps) {
  return (
    <div
      className="metric-chip"
      style={borderColor ? { borderColor } : undefined}
    >
      <span
        className="metric-value"
        style={valueColor ? { color: valueColor } : undefined}
      >
        {value}
      </span>
      <span className="metric-label">{label}</span>
    </div>
  );
}
