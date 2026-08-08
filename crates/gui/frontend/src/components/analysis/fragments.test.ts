import { describe, expect, it } from "vitest";
import {
  type ChartShape,
  chartBars,
  formatValue,
  lineSeriesView,
} from "./fragments";

/**
 * The pure decisions of rendering a fragment. The fragments themselves are
 * engine-authored and display-resolved before they arrive, so what is
 * tested here is only what this layer adds: text formatting and the
 * reshaping of chart data for the drawing primitives.
 */

describe("formatValue", () => {
  it("scales precision with magnitude and keeps resolved units verbatim", () => {
    expect(formatValue({ type: "number", value: 1234.56, unit: "m³" })).toBe(
      "1235 m³",
    );
    expect(formatValue({ type: "number", value: 12.345, unit: "psi" })).toBe(
      "12.35 psi",
    );
    expect(formatValue({ type: "number", value: 0.01234 })).toBe("0.0123");
  });

  it("renders every non-number kind", () => {
    expect(formatValue({ type: "integer", value: 12500 })).toBe("12,500");
    expect(formatValue({ type: "boolean", value: true })).toBe("Yes");
    expect(formatValue({ type: "text", value: "Chlorine" })).toBe("Chlorine");
    expect(formatValue({ type: "absent" })).toBe("—");
  });
});

describe("chartBars", () => {
  const bar = (values: number[]): ChartShape => ({
    xLabel: "Band",
    yLabel: "Junctions",
    data: {
      type: "bar",
      categories: values.map((_, i) => `b${i}`),
      values,
    },
  });

  it("pairs categories with values and scales to the tallest", () => {
    const { bars, max } = chartBars(bar([3, 7, 2]));
    expect(bars.map((b) => b.count)).toEqual([3, 7, 2]);
    expect(max).toBe(7);
  });

  /** Every band empty is a legitimate chart (thresholds nobody violates);
   * the scale clamps to 1 so the geometry divides by something. */
  it("an all-zero chart keeps a nonzero scale", () => {
    const { max } = chartBars(bar([0, 0]));
    expect(max).toBe(1);
  });

  it("a line chart yields no bars", () => {
    expect(
      chartBars({
        xLabel: "t",
        yLabel: "y",
        data: { type: "line", series: [] },
      }).bars,
    ).toEqual([]);
  });
});

describe("lineSeriesView", () => {
  const line = (xUnit?: string): ChartShape => ({
    xLabel: "Time",
    xUnit,
    yLabel: "Head",
    yUnit: "m",
    data: {
      type: "line",
      series: [
        {
          name: "T1",
          points: [
            [0, 10],
            [1, 12],
          ],
        },
        {
          name: "T2",
          points: [
            [0, 95],
            [1, 96],
          ],
        },
      ],
    },
  });

  /** Each series scales to its own range: tank heads at different
   * elevations sit in disjoint bands, and one shared scale would flatten
   * all but the widest. */
  it("gives each series its own range", () => {
    const view = lineSeriesView(line("h"));
    expect(view[0]).toMatchObject({ name: "T1", min: 10, max: 12 });
    expect(view[1]).toMatchObject({ name: "T2", min: 95, max: 96 });
  });

  it("converts an hours axis to seconds for the time readout", () => {
    expect(lineSeriesView(line("h"))[0].times).toEqual([0, 3600]);
    expect(lineSeriesView(line(undefined))[0].times).toBeUndefined();
  });
});
