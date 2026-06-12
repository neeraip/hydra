// ── Shared chart primitives ───────────────────────────────────────────────────

export function NoDataCard({ message }: { message: string }) {
  return (
    <div
      style={{
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius: 10,
        padding: 24,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        color: "var(--text-tertiary)",
        fontSize: 13,
      }}
    >
      {message}
    </div>
  );
}

export function AuditMetric({
  value,
  label,
  valueColor,
}: {
  value: string;
  label: string;
  valueColor?: string;
}) {
  return (
    <div
      style={{
        background: "var(--bg-panel)",
        border: "1px solid var(--border)",
        borderRadius: 7,
        padding: "8px 10px",
      }}
    >
      <div
        style={{
          fontSize: 14,
          fontWeight: 600,
          color: valueColor ?? "var(--text-primary)",
          fontFamily: "var(--font-mono)",
        }}
      >
        {value}
      </div>
      <div
        style={{ fontSize: 11, color: "var(--text-tertiary)", marginTop: 2 }}
      >
        {label}
      </div>
    </div>
  );
}

export function Sparkline({
  values,
  min,
  max,
  stroke,
}: {
  values: number[];
  min: number;
  max: number;
  stroke: string;
}) {
  const w = 280,
    h = 36;
  const pts = values
    .map((v, i) => {
      const x = (i / Math.max(values.length - 1, 1)) * w;
      const y = h - ((v - min) / Math.max(max - min, 1e-9)) * h;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  return (
    <svg
      width="100%"
      viewBox={`0 0 ${w} ${h}`}
      preserveAspectRatio="none"
      style={{ height: 36 }}
    >
      <title>Trend sparkline</title>
      <polyline points={pts} fill="none" stroke={stroke} strokeWidth={1.5} />
    </svg>
  );
}

export interface BarEntry {
  label: string;
  count: number;
  fill: string;
}

export function HorizontalBarChart({
  bars,
  maxCount,
}: {
  bars: BarEntry[];
  maxCount: number;
}) {
  const rowH = 22;
  const labelW = 88;
  const barAreaW = 240;
  const height = bars.length * rowH;

  return (
    <svg
      width="100%"
      viewBox={`0 0 ${labelW + barAreaW + 24} ${height}`}
      style={{ overflow: "visible" }}
    >
      <title>Horizontal bar chart</title>
      {bars.map((bar, i) => {
        const y = i * rowH;
        const barW = maxCount > 0 ? (bar.count / maxCount) * barAreaW : 0;
        return (
          <g key={bar.label}>
            <text
              x={labelW - 6}
              y={y + rowH / 2 + 4}
              textAnchor="end"
              fontSize={10}
              fill="var(--text-tertiary)"
              style={{ fontFamily: "var(--font-mono)" }}
            >
              {bar.label}
            </text>
            <rect
              x={labelW}
              y={y + 5}
              width={Math.max(barW, bar.count > 0 ? 2 : 0)}
              height={rowH - 10}
              rx={2}
              fill={bar.fill}
            />
            {bar.count > 0 && (
              <text
                x={labelW + barW + 5}
                y={y + rowH / 2 + 4}
                fontSize={10}
                fill="var(--text-secondary)"
                style={{ fontFamily: "var(--font-mono)" }}
              >
                {bar.count}
              </text>
            )}
          </g>
        );
      })}
    </svg>
  );
}
