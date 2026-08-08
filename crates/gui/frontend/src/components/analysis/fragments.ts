/** Fragment wire shapes (hydra-common §3.3, serde camelCase) and the pure
 * decisions of rendering them — shared by every engine's analysis view.
 *
 * Fragments arrive from `get_analysis_blocks` already display-resolved
 * (report spec §4.0): the backend converted quantity-tagged values into
 * the reader's unit system and dropped the tags, so nothing here converts
 * anything. This module renders shapes, never meanings — an engine's
 * vocabulary stays in the engine.
 */

export type Value =
  | { type: "number"; value: number; unit?: string }
  | { type: "integer"; value: number }
  | { type: "boolean"; value: boolean }
  | { type: "text"; value: string }
  | { type: "timestamp"; value: string }
  | { type: "absent" };

export interface TableShape {
  columns: Array<{ name: string; unit?: string; kind: string }>;
  rows: Value[][];
}

export interface ChartShape {
  xLabel: string;
  xUnit?: string;
  yLabel: string;
  yUnit?: string;
  data:
    | { type: "bar"; categories: string[]; values: number[] }
    | {
        type: "line";
        series: Array<{ name: string; points: [number, number][] }>;
      };
}

export type FragmentItem =
  | { type: "keyValues"; entries: Array<{ label: string; value: Value }> }
  | { type: "table"; table: TableShape }
  | { type: "note"; text: string }
  | { type: "chart"; chart: ChartShape };

export interface Fragment {
  title: string;
  items: FragmentItem[];
}

export interface AnalysisBlock {
  id: string;
  title: string;
  status: "ok" | "unavailable" | "failed";
  reason?: string;
  fragment?: Fragment;
}

/** One value as display text. Numbers keep engine-resolved units verbatim;
 * precision scales with magnitude, since a fragment value can be anything
 * from a closure percentage to a reservoir volume. */
export function formatValue(v: Value): string {
  switch (v.type) {
    case "number": {
      const n =
        Math.abs(v.value) >= 100
          ? v.value.toFixed(0)
          : Math.abs(v.value) >= 1
            ? v.value.toFixed(2)
            : v.value.toPrecision(3);
      return v.unit ? `${n} ${v.unit}` : n;
    }
    case "integer":
      return v.value.toLocaleString();
    case "boolean":
      return v.value ? "Yes" : "No";
    case "text":
    case "timestamp":
      return v.value;
    case "absent":
      return "—";
  }
}

/** A bar chart's rows for `HorizontalBarChart`, and the scale they share.
 *
 * `max` comes from the data rather than being threaded separately so an
 * all-zero chart (every band empty) divides by 1, not 0, and renders as
 * empty tracks rather than NaN geometry. */
export function chartBars(chart: ChartShape): {
  bars: Array<{ label: string; count: number }>;
  max: number;
} {
  if (chart.data.type !== "bar") return { bars: [], max: 1 };
  const bars = chart.data.categories.map((label, i) => ({
    label,
    count: chart.data.type === "bar" ? (chart.data.values[i] ?? 0) : 0,
  }));
  return { bars, max: Math.max(1, ...bars.map((b) => b.count)) };
}

/** A line chart's series in the shape `Sparkline` draws: values with their
 * own min/max, and per-point times in seconds when the x axis is hours —
 * which is what every engine's time axis emits (`xUnit: "h"`/`"hr"`).
 * Sparse series (a point missing at some x) keep their own x order; each
 * series scales to its own range, since a fragment's series (tank heads at
 * different elevations) can sit in disjoint bands. */
export function lineSeriesView(chart: ChartShape): Array<{
  name: string;
  values: number[];
  min: number;
  max: number;
  times?: number[];
}> {
  if (chart.data.type !== "line") return [];
  const hoursAxis = chart.xUnit === "h" || chart.xUnit === "hr";
  return chart.data.series.map((s) => {
    const values = s.points.map((p) => p[1]);
    return {
      name: s.name,
      values,
      min: Math.min(...values),
      max: Math.max(...values),
      times: hoursAxis ? s.points.map((p) => p[0] * 3600) : undefined,
    };
  });
}
