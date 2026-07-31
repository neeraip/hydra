import type { HistogramBucket, ResultAnalytics } from "../../../hooks";
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
// Labels are derived from the buckets themselves, so nothing here needs to
// stay in step with PRESSURE_BINS / VELOCITY_BINS in the backend's
// commands/results.rs.

function fmtEdge(v: number, q: Quantity, sys: UnitSystem): string {
  const conv = toDisplay(v, q, sys);
  // Compact labels: whole numbers where possible, one decimal otherwise.
  return Number.isInteger(Number(conv.toFixed(1)))
    ? String(Math.round(conv))
    : conv.toFixed(1);
}

/** Labels derived from the buckets the backend actually produced, rather than
 *  from a copy of the band edges kept here.  The criteria live in one place
 *  (the engine's threshold bands, analysis spec §4.1.2); duplicating them in
 *  TypeScript meant a backend edge change silently mislabelled every bar.
 *
 *  The outer bands are unbounded — the leading bucket is where junctions in
 *  pressure deficit land, and dropping it removed them from the chart
 *  entirely — so they render as "< x" and "≥ y" from their finite side. */
function binLabels(
  buckets: HistogramBucket[],
  q: Quantity,
  sys: UnitSystem,
  topPrefix: "≥" | ">",
): string[] {
  const u = unitLabel(q, sys);
  return buckets.map((b, i) => {
    if (i === 0) return `< ${fmtEdge(b.hi, q, sys)} ${u}`;
    if (i === buckets.length - 1)
      return `${topPrefix} ${fmtEdge(b.lo, q, sys)} ${u}`;
    return `${fmtEdge(b.lo, q, sys)}–${fmtEdge(b.hi, q, sys)} ${u}`;
  });
}

export function PressureHistogram({
  analytics,
  minPressureM,
}: {
  analytics: ResultAnalytics | null;
  /** The user's minimum-service-pressure criterion (m, SI) — the same value
   *  `lowPressureCount` was computed against. */
  minPressureM: number;
}) {
  const sys = useUnitSystem();
  if (!analytics) {
    return (
      <div className="insights-card">
        <span
          style={{
            fontSize: "var(--text-lg)",
            fontWeight: 500,
            color: "var(--text-primary)",
          }}
        >
          Pressure Adequacy
        </span>
        <div
          style={{
            marginTop: 16,
            color: "var(--text-tertiary)",
            fontSize: "var(--text-lg)",
          }}
        >
          Run a simulation to see the pressure distribution.
        </div>
      </div>
    );
  }

  const { pressureHistogram, junctionCount } = analytics;
  const labels = binLabels(pressureHistogram, "pressure", sys, "≥");
  const maxCount = Math.max(...pressureHistogram.map((b) => b.count), 1);
  // Colour by the bucket's own range against the user's criterion, not by a
  // fixed index: a bucket entirely below the criterion is an error, one that
  // straddles it is marginal, one entirely above it is fine. Index-based
  // colouring silently lied whenever the criterion moved off its default.
  const bars: BarEntry[] = pressureHistogram.map((b, i) => ({
    label: labels[i] ?? `Bin ${i}`,
    count: b.count,
    fill:
      b.hi <= minPressureM
        ? "var(--status-error)"
        : b.lo < minPressureM
          ? "var(--status-warning)"
          : b.count > 0
            ? "var(--accent)"
            : "var(--border)",
  }));

  const belowThreshold = analytics.lowPressureCount;
  // The criterion the count was actually computed against — previously a
  // fixed 24 m constant, which contradicted the criterion shown one line
  // above it in the same panel.
  const thresholdLabel = formatQty(
    minPressureM,
    "pressure",
    sys,
    sys === "si" ? 0 : 1,
  );

  return (
    <div className="insights-card">
      <div style={{ display: "flex", alignItems: "center", marginBottom: 12 }}>
        <span
          style={{
            fontSize: "var(--text-lg)",
            fontWeight: 500,
            color: "var(--text-primary)",
            flex: 1,
          }}
        >
          Pressure Adequacy
        </span>
        <span
          style={{
            fontSize: "var(--text-xs)",
            background: "var(--bg-app)",
            border: "1px solid var(--border)",
            borderRadius: 10,
            padding: "2px 8px",
            color: "var(--text-tertiary)",
            fontFamily: "var(--font-mono)",
          }}
        >
          {junctionCount} junctions
        </span>
      </div>
      <HorizontalBarChart bars={bars} maxCount={maxCount} />
      <div
        style={{
          fontSize: "var(--text-md)",
          color: "var(--text-tertiary)",
          marginTop: 10,
        }}
      >
        {belowThreshold > 0
          ? `${belowThreshold} junction${belowThreshold > 1 ? "s" : ""} below minimum (${thresholdLabel}) at worst hour`
          : `All junctions above the minimum pressure criterion (${thresholdLabel})`}
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
            fontSize: "var(--text-lg)",
            fontWeight: 500,
            color: "var(--text-primary)",
          }}
        >
          Velocity Distribution
        </span>
        <div
          style={{
            marginTop: 16,
            color: "var(--text-tertiary)",
            fontSize: "var(--text-lg)",
          }}
        >
          Run a simulation to see the velocity distribution.
        </div>
      </div>
    );
  }

  const { velocityHistogram, pipeCount } = analytics;
  const labels = binLabels(velocityHistogram, "velocity", sys, ">");
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
            fontSize: "var(--text-lg)",
            fontWeight: 500,
            color: "var(--text-primary)",
            flex: 1,
          }}
        >
          Velocity Distribution
        </span>
        <span
          style={{
            fontSize: "var(--text-xs)",
            background: "var(--bg-app)",
            border: "1px solid var(--border)",
            borderRadius: 10,
            padding: "2px 8px",
            color: "var(--text-tertiary)",
            fontFamily: "var(--font-mono)",
          }}
        >
          {pipeCount} pipes
        </span>
      </div>
      <HorizontalBarChart bars={bars} maxCount={maxCount} />
      <div
        style={{
          fontSize: "var(--text-md)",
          color: "var(--text-tertiary)",
          marginTop: 10,
        }}
      >
        {highVelocityCount > 0
          ? `${highVelocityCount} pipe${highVelocityCount > 1 ? "s" : ""} exceed ${highVelLabel}; check for head loss`
          : "All pipes within acceptable velocity range"}
      </div>
    </div>
  );
}
