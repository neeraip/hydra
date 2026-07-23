// ── Shared chart primitives ───────────────────────────────────────────────────

import { type CSSProperties, useState } from "react";

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

/** Format a simulation time in seconds as `h:mm` (hours may exceed 24). */
export function formatSimTime(seconds: number): string {
  const totalMinutes = Math.round(seconds / 60);
  const h = Math.floor(totalMinutes / 60);
  const m = totalMinutes % 60;
  return `${h}:${String(m).padStart(2, "0")}`;
}

/** Compact numeric label for axis bounds / hover readouts. */
function formatChartValue(v: number, decimals: number): string {
  if (!Number.isFinite(v)) return "—";
  const abs = Math.abs(v);
  if (abs >= 1000) return v.toFixed(0);
  return v.toFixed(decimals);
}

/**
 * Compact trend chart.
 *
 * Base form (no `times`): the original static polyline sparkline.
 * With `times` (seconds per point) it becomes interactive: axis min/max
 * labels, a hover readout of `(time, value)`, and an optional vertical
 * marker at `markerIndex` (e.g. the currently scrubbed period).
 */
export function Sparkline({
  values,
  min,
  max,
  stroke,
  times,
  markerIndex,
  unit,
  decimals = 2,
  height = 36,
}: {
  values: number[];
  min: number;
  max: number;
  stroke: string;
  /** Per-point times in seconds; enables hover readout + axis labels. */
  times?: number[];
  /** Index of the currently scrubbed period; draws a vertical marker. */
  markerIndex?: number | null;
  /** Display unit appended to the hover readout (e.g. "m", "L/s"). */
  unit?: string;
  /** Decimal places for value labels. */
  decimals?: number;
  height?: number;
}) {
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);
  const w = 280;
  const h = height;
  const span = Math.max(max - min, 1e-9);
  const xAt = (i: number) => (i / Math.max(values.length - 1, 1)) * w;
  const yAt = (v: number) => h - ((v - min) / span) * h;
  const pts = values
    .map((v, i) => `${xAt(i).toFixed(1)},${yAt(v).toFixed(1)}`)
    .join(" ");

  const interactive = times != null && values.length > 0;

  const chart = (
    <svg
      width="100%"
      viewBox={`0 0 ${w} ${h}`}
      preserveAspectRatio="none"
      style={{ height: h, display: "block" }}
      onPointerMove={
        interactive
          ? (e) => {
              const rect = e.currentTarget.getBoundingClientRect();
              const frac = (e.clientX - rect.left) / Math.max(rect.width, 1);
              const idx = Math.min(
                values.length - 1,
                Math.max(0, Math.round(frac * (values.length - 1))),
              );
              setHoverIdx(idx);
            }
          : undefined
      }
      onPointerLeave={interactive ? () => setHoverIdx(null) : undefined}
    >
      <title>Trend sparkline</title>
      {markerIndex != null &&
        markerIndex >= 0 &&
        markerIndex < values.length && (
          <line
            x1={xAt(markerIndex)}
            y1={0}
            x2={xAt(markerIndex)}
            y2={h}
            stroke="var(--text-tertiary)"
            strokeWidth={1}
            strokeDasharray="3 3"
          />
        )}
      <polyline points={pts} fill="none" stroke={stroke} strokeWidth={1.5} />
      {interactive && hoverIdx != null && (
        <>
          <line
            x1={xAt(hoverIdx)}
            y1={0}
            x2={xAt(hoverIdx)}
            y2={h}
            stroke="var(--border-hover)"
            strokeWidth={1}
          />
          <circle
            cx={xAt(hoverIdx)}
            cy={yAt(values[hoverIdx])}
            r={2.5}
            fill={stroke}
          />
        </>
      )}
    </svg>
  );

  if (!interactive) return chart;

  const axisLabel: CSSProperties = {
    position: "absolute",
    left: 2,
    fontSize: 9,
    lineHeight: 1,
    color: "var(--text-tertiary)",
    fontFamily: "var(--font-mono)",
    pointerEvents: "none",
    background: "color-mix(in srgb, var(--bg-card) 70%, transparent)",
    padding: "0 2px",
    borderRadius: 2,
  };

  return (
    <div style={{ position: "relative" }}>
      {chart}
      <span style={{ ...axisLabel, top: 0 }}>
        {formatChartValue(max, decimals)}
      </span>
      <span style={{ ...axisLabel, bottom: 0 }}>
        {formatChartValue(min, decimals)}
      </span>
      {hoverIdx != null && times?.[hoverIdx] != null && (
        <span
          style={{
            ...axisLabel,
            left: "auto",
            right: 2,
            top: 0,
            color: "var(--text-secondary)",
          }}
        >
          {formatSimTime(times[hoverIdx])} ·{" "}
          {formatChartValue(values[hoverIdx], decimals)}
          {unit ? ` ${unit}` : ""}
        </span>
      )}
    </div>
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
