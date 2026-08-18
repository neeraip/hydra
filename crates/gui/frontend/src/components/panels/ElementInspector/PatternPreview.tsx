import { useMemo } from "react";
import { useAppState } from "../../../AppContext";
import { useCollectionDetail } from "../../../hooks";
import { Sparkline } from "../../../pages/project/AnalysisPanel/charts";
import { downsampleMinMax } from "./patternDownsample";

/**
 * Points beyond which the profile is downsampled.
 *
 * The sparkline is 280 units wide, so a year of hourly multipliers (8,760
 * points) would put thirty vertices in every pixel. `downsampleMinMax` keeps
 * each bucket's extremes, so peaks survive the reduction instead of being
 * averaged away — the shape is the entire reason this chart is here.
 */
const MAX_POINTS = 140;

/**
 * Profile of a referenced pattern, shown beneath the reference itself.
 *
 * A pattern is a repeating list of dimensionless multipliers (model spec §2.2);
 * an id alone says which one without saying what it does. The inspector's
 * time-series card plots the *outcome* — and only once a multi-period
 * simulation has run — whereas this is the *input*, so it is readable on a
 * network that has never been simulated, on a steady-state run, and when the
 * two disagree because a pressure-dependent demand model delivered less than
 * was asked for.
 *
 * Reads the one pattern it names, through the same command the editor's
 * contents table uses. It used to ask for every pattern in the model
 * through `get_patterns`, which no backend ever defined: the invoke
 * failed, the wrapper turned the failure into an empty list, and the
 * preview silently took the branch meant for a dangling reference. So
 * this chart has never drawn. Fetching one instead of all is also the
 * cheaper read, since a pattern can hold a multiplier per hour of a year.
 */
export function PatternPreview({
  patternId,
  stroke,
}: {
  patternId: string;
  stroke: string;
}) {
  const { activeProjectId, activeScenarioId } = useAppState();
  const { detail } = useCollectionDetail(
    activeProjectId,
    activeScenarioId,
    "pattern",
    patternId,
  );

  const chart = useMemo(() => {
    // Both engines serve a pattern as [Interval, Factor]; the multiplier
    // is the second column, and the first only counts the rows.
    const values = detail.rows.map((r) => r[1]);
    if (values.length === 0) return null;
    // A loop, not `Math.min(...values)`: patterns run to a value per hour for a
    // year, and spreading that many arguments is a stack overflow away.
    let min = values[0];
    let max = values[0];
    for (const v of values) {
      if (v < min) min = v;
      if (v > max) max = v;
    }
    const points =
      values.length > MAX_POINTS
        ? downsampleMinMax(values, MAX_POINTS).map((b) => (b.min + b.max) / 2)
        : values;
    return { points, min, max, steps: values.length };
  }, [detail]);

  // An id that resolves to nothing is a dangling reference, which validation
  // reports (`UnknownPatternRef`) — drawing an empty chart would imply the
  // pattern exists and is flat.
  if (!chart) return null;

  return (
    <div style={{ paddingTop: 2 }}>
      <Sparkline
        values={chart.points}
        min={chart.min}
        max={chart.max}
        stroke={stroke}
        height={30}
      />
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          fontSize: "var(--text-2xs)",
          color: "var(--text-secondary)",
          fontFamily: "var(--font-mono)",
          marginTop: 1,
        }}
      >
        <span>×{chart.min.toFixed(2)}</span>
        <span style={{ color: "var(--text-tertiary)" }}>
          {chart.steps} step{chart.steps === 1 ? "" : "s"}
        </span>
        <span>×{chart.max.toFixed(2)}</span>
      </div>
    </div>
  );
}
