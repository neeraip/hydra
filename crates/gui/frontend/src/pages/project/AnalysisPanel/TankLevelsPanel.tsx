import { useSimulation } from "../../../AppContext";
import { SectionHeader } from "../../../components/ui/SectionHeader";
import type { ResultAnalytics } from "../../../hooks";
import {
  defaultDecimals,
  toDisplay,
  unitLabel,
  useUnitSystem,
} from "../../../units";
import { NoDataCard, Sparkline } from "./charts";

/**
 * Cap on rendered tank sparklines — networks can carry hundreds of tanks and
 * the card is a summary, not a browser. Remaining tanks are counted in a
 * "+N more" note.
 */
const MAX_TANK_SPARKLINES = 8;

/**
 * Min/max bounds for one tank's display series. Flat series (a tank that
 * never moves) are padded so the polyline sits mid-chart instead of hugging
 * an edge with two identical axis labels.
 */
function seriesBounds(values: number[]): { min: number; max: number } {
  let min = Infinity;
  let max = -Infinity;
  for (const v of values) {
    if (v < min) min = v;
    if (v > max) max = v;
  }
  if (!Number.isFinite(min) || !Number.isFinite(max)) return { min: 0, max: 1 };
  if (max - min < 1e-6) return { min: min - 0.5, max: max + 0.5 };
  return { min, max };
}

/** Per-tank hydraulic-head trends over the simulation horizon. */
export function TankLevelsPanel({
  analytics,
}: {
  analytics: ResultAnalytics | null;
}) {
  const sys = useUnitSystem();
  const { resultMeta } = useSimulation();

  if (!analytics) {
    return (
      <div>
        <SectionHeader>Tank Levels</SectionHeader>
        <NoDataCard message="Run a simulation to see tank levels." />
      </div>
    );
  }
  const tanks = analytics.tankSeries;
  if (tanks.length === 0) {
    return (
      <div>
        <SectionHeader>Tank Levels</SectionHeader>
        <NoDataCard message="No tanks in this network." />
      </div>
    );
  }

  const shown = tanks.slice(0, MAX_TANK_SPARKLINES);
  const hiddenCount = tanks.length - shown.length;
  const unit = unitLabel("head", sys);
  const decimals = defaultDecimals("head", sys);
  const times = resultMeta?.times;

  return (
    <div>
      <SectionHeader>Tank Levels</SectionHeader>
      <div
        style={{
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          padding: 16,
        }}
      >
        <div
          style={{
            fontSize: 11,
            color: "var(--text-tertiary)",
            marginBottom: 10,
          }}
        >
          Hydraulic head over the simulation horizon ({unit})
        </div>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
            gap: "14px 20px",
          }}
        >
          {shown.map((tank) => {
            // Heads arrive in SI metres; convert once for display.
            const values = tank.head.map((v) => toDisplay(v, "head", sys));
            const { min, max } = seriesBounds(values);
            return (
              <div key={tank.nodeId}>
                <div
                  style={{
                    fontSize: 11,
                    fontFamily: "var(--font-mono)",
                    color: "var(--text-secondary)",
                    marginBottom: 4,
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {tank.nodeId}
                </div>
                {values.length > 0 ? (
                  <Sparkline
                    values={values}
                    min={min}
                    max={max}
                    stroke="var(--accent)"
                    // Only enable the hover/time layer when the snapshot
                    // times actually line up with this series.
                    times={
                      times && times.length === values.length
                        ? times
                        : undefined
                    }
                    unit={unit}
                    decimals={decimals}
                  />
                ) : (
                  <div style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
                    No data
                  </div>
                )}
              </div>
            );
          })}
        </div>
        {hiddenCount > 0 && (
          <div
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              marginTop: 12,
            }}
          >
            +{hiddenCount} more tank{hiddenCount > 1 ? "s" : ""} not shown
          </div>
        )}
      </div>
    </div>
  );
}
