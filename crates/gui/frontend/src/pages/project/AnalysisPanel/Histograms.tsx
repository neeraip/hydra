import type { ResultAnalytics } from "../../../hooks";
import { PRESSURE_THRESHOLD } from "../../../hooks";
import { type BarEntry, HorizontalBarChart } from "./charts";

const PRESSURE_BIN_LABELS = [
  "0–10 m",
  "10–20 m",
  "20–30 m",
  "30–40 m",
  "40–50 m",
  "50–60 m",
  "≥ 60 m",
];
const VELOCITY_BIN_LABELS = [
  "< 0.1 m/s",
  "0.1–0.3 m/s",
  "0.3–0.6 m/s",
  "0.6–1.0 m/s",
  "> 1.0 m/s",
];

export function PressureHistogram({
  analytics,
}: {
  analytics: ResultAnalytics | null;
}) {
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
  const data = pressureHistogram.map((b, i) => ({
    label: PRESSURE_BIN_LABELS[i] ?? `Bin ${i}`,
    count: b.count,
  }));
  const maxCount = Math.max(...data.map((b) => b.count), 1);
  const bars: BarEntry[] = data.map((b) => ({
    label: b.label,
    count: b.count,
    fill:
      b.label === "20–30 m"
        ? "var(--status-warning)"
        : b.label === "0–10 m" || b.label === "10–20 m"
          ? "var(--status-error)"
          : b.count > 0
            ? "var(--accent)"
            : "var(--border)",
  }));

  const belowThreshold = analytics.lowPressureCount;

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
          ? `${belowThreshold} node${belowThreshold > 1 ? "s" : ""} below minimum (${PRESSURE_THRESHOLD} m) at worst hour`
          : `All nodes above minimum pressure threshold (${PRESSURE_THRESHOLD} m)`}
      </div>
    </div>
  );
}

export function VelocityHistogram({
  analytics,
}: {
  analytics: ResultAnalytics | null;
}) {
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
  const data = velocityHistogram.map((b, i) => ({
    label: VELOCITY_BIN_LABELS[i] ?? `Bin ${i}`,
    count: b.count,
  }));
  const maxCount = Math.max(...data.map((b) => b.count), 1);
  const fillMap: Record<string, string> = {
    "< 0.1 m/s": "var(--text-tertiary)",
    "0.1–0.3 m/s": "var(--status-success)",
    "0.3–0.6 m/s": "var(--accent)",
    "0.6–1.0 m/s": "var(--accent)",
    "> 1.0 m/s": "var(--status-warning)",
  };
  const bars: BarEntry[] = data.map((b) => ({
    label: b.label,
    count: b.count,
    fill: fillMap[b.label] ?? "var(--border)",
  }));
  const highVelocityCount =
    data.find((b) => b.label === "> 1.0 m/s")?.count ?? 0;

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
          ? `${highVelocityCount} pipe${highVelocityCount > 1 ? "s" : ""} exceed 1.0 m/s; check for head loss`
          : "All pipes within acceptable velocity range"}
      </div>
    </div>
  );
}
