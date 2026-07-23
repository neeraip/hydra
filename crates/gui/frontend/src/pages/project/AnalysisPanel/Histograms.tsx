import type { ResultAnalytics } from "../../../hooks";
import { PRESSURE_THRESHOLD } from "../../../hooks";
import {
  formatQty,
  type Quantity,
  toDisplay,
  type UnitSystem,
  unitLabel,
  useUnitSystem,
} from "../../../units";
import { type BarEntry, HorizontalBarChart } from "./charts";

// Histogram buckets arrive from the backend with fixed SI boundaries; only
// the labels are converted for display — counts and bucket edges stay SI.
const PRESSURE_BIN_EDGES_M = [0, 10, 20, 30, 40, 50, 60];
const VELOCITY_BIN_EDGES_MS = [0.1, 0.3, 0.6, 1.0];

function fmtEdge(v: number, q: Quantity, sys: UnitSystem): string {
  const conv = toDisplay(v, q, sys);
  // Compact labels: whole numbers where possible, one decimal otherwise.
  return Number.isInteger(Number(conv.toFixed(1)))
    ? String(Math.round(conv))
    : conv.toFixed(1);
}

/** "0–10 m", …, "≥ 60 m" (converted for the active display system). */
function pressureBinLabels(sys: UnitSystem): string[] {
  const u = unitLabel("pressure", sys);
  const e = PRESSURE_BIN_EDGES_M.map((v) => fmtEdge(v, "pressure", sys));
  const labels: string[] = [];
  for (let i = 0; i < e.length - 1; i += 1)
    labels.push(`${e[i]}–${e[i + 1]} ${u}`);
  labels.push(`≥ ${e[e.length - 1]} ${u}`);
  return labels;
}

/** "< 0.1 m/s", …, "> 1.0 m/s" (converted for the active display system). */
function velocityBinLabels(sys: UnitSystem): string[] {
  const u = unitLabel("velocity", sys);
  const e = VELOCITY_BIN_EDGES_MS.map((v) => fmtEdge(v, "velocity", sys));
  const labels: string[] = [`< ${e[0]} ${u}`];
  for (let i = 0; i < e.length - 1; i += 1)
    labels.push(`${e[i]}–${e[i + 1]} ${u}`);
  labels.push(`> ${e[e.length - 1]} ${u}`);
  return labels;
}

export function PressureHistogram({
  analytics,
}: {
  analytics: ResultAnalytics | null;
}) {
  const sys = useUnitSystem();
  if (!analytics) {
    return (
      <div className="insights-card">
        <span
          style={{
            fontSize: 13,
            fontWeight: 500,
            color: "var(--text-primary)",
          }}
        >
          Pressure Adequacy
        </span>
        <div
          style={{ marginTop: 16, color: "var(--text-tertiary)", fontSize: 13 }}
        >
          Run a simulation to see the pressure distribution.
        </div>
      </div>
    );
  }

  const { pressureHistogram, nodeCount } = analytics;
  const labels = pressureBinLabels(sys);
  const maxCount = Math.max(...pressureHistogram.map((b) => b.count), 1);
  // Colour by bucket index (SI semantics): bins 0–1 (< 20 m) are below the
  // pressure threshold, bin 2 (20–30 m) is marginal.
  const bars: BarEntry[] = pressureHistogram.map((b, i) => ({
    label: labels[i] ?? `Bin ${i}`,
    count: b.count,
    fill:
      i <= 1
        ? "var(--status-error)"
        : i === 2
          ? "var(--status-warning)"
          : b.count > 0
            ? "var(--accent)"
            : "var(--border)",
  }));

  const belowThreshold = analytics.lowPressureCount;
  const thresholdLabel = formatQty(
    PRESSURE_THRESHOLD,
    "pressure",
    sys,
    sys === "si" ? 0 : 1,
  );

  return (
    <div className="insights-card">
      <div style={{ display: "flex", alignItems: "center", marginBottom: 12 }}>
        <span
          style={{
            fontSize: 13,
            fontWeight: 500,
            color: "var(--text-primary)",
            flex: 1,
          }}
        >
          Pressure Adequacy
        </span>
        <span
          style={{
            fontSize: 10,
            background: "var(--bg-app)",
            border: "1px solid var(--border)",
            borderRadius: 10,
            padding: "2px 8px",
            color: "var(--text-tertiary)",
            fontFamily: "var(--font-mono)",
          }}
        >
          {nodeCount} nodes
        </span>
      </div>
      <HorizontalBarChart bars={bars} maxCount={maxCount} />
      <div
        style={{ fontSize: 12, color: "var(--text-tertiary)", marginTop: 10 }}
      >
        {belowThreshold > 0
          ? `${belowThreshold} node${belowThreshold > 1 ? "s" : ""} below minimum (${thresholdLabel}) at worst hour`
          : `All nodes above minimum pressure threshold (${thresholdLabel})`}
      </div>
    </div>
  );
}

export function VelocityHistogram({
  analytics,
}: {
  analytics: ResultAnalytics | null;
}) {
  const sys = useUnitSystem();
  if (!analytics) {
    return (
      <div className="insights-card">
        <span
          style={{
            fontSize: 13,
            fontWeight: 500,
            color: "var(--text-primary)",
          }}
        >
          Velocity Distribution
        </span>
        <div
          style={{ marginTop: 16, color: "var(--text-tertiary)", fontSize: 13 }}
        >
          Run a simulation to see the velocity distribution.
        </div>
      </div>
    );
  }

  const { velocityHistogram, linkCount } = analytics;
  const labels = velocityBinLabels(sys);
  const maxCount = Math.max(...velocityHistogram.map((b) => b.count), 1);
  // Colour by bucket index: stagnant / good / normal / normal / too fast.
  const fillByIndex = [
    "var(--text-tertiary)",
    "var(--status-success)",
    "var(--accent)",
    "var(--accent)",
    "var(--status-warning)",
  ];
  const bars: BarEntry[] = velocityHistogram.map((b, i) => ({
    label: labels[i] ?? `Bin ${i}`,
    count: b.count,
    fill: fillByIndex[i] ?? "var(--border)",
  }));
  const highVelocityCount =
    velocityHistogram[velocityHistogram.length - 1]?.count ?? 0;
  const highVelLabel = formatQty(1.0, "velocity", sys, 1);

  return (
    <div className="insights-card">
      <div style={{ display: "flex", alignItems: "center", marginBottom: 12 }}>
        <span
          style={{
            fontSize: 13,
            fontWeight: 500,
            color: "var(--text-primary)",
            flex: 1,
          }}
        >
          Velocity Distribution
        </span>
        <span
          style={{
            fontSize: 10,
            background: "var(--bg-app)",
            border: "1px solid var(--border)",
            borderRadius: 10,
            padding: "2px 8px",
            color: "var(--text-tertiary)",
            fontFamily: "var(--font-mono)",
          }}
        >
          {linkCount} pipes
        </span>
      </div>
      <HorizontalBarChart bars={bars} maxCount={maxCount} />
      <div
        style={{ fontSize: 12, color: "var(--text-tertiary)", marginTop: 10 }}
      >
        {highVelocityCount > 0
          ? `${highVelocityCount} pipe${highVelocityCount > 1 ? "s" : ""} exceed ${highVelLabel}; check for head loss`
          : "All pipes within acceptable velocity range"}
      </div>
    </div>
  );
}
