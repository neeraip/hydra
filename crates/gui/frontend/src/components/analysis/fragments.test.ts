import { describe, expect, it } from "vitest";
import {
  type AnalysisBlock,
  activeCategoryOf,
  blockSpan,
  type ChartShape,
  categoriesOf,
  chartBars,
  type Fragment,
  formatValue,
  layoutSpans,
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

describe("blockSpan", () => {
  const kv = (n: number) => ({
    type: "keyValues" as const,
    entries: Array.from({ length: n }, (_, i) => ({
      label: `k${i}`,
      value: { type: "integer" as const, value: i },
    })),
  });
  const table = (cols: number) => ({
    type: "table" as const,
    table: {
      columns: Array.from({ length: cols }, (_, i) => ({
        name: `c${i}`,
        kind: "number",
      })),
      rows: [],
    },
  });

  /** The decision the grid renders from: wide content claims the row,
   * compact content shares one. Sized from the fragment's own shape —
   * an engine hint would put presentation into the neutral layer. */
  it("wide tables and long key-value lists claim the full row", () => {
    expect(blockSpan({ title: "t", items: [table(5)] })).toBe("full");
    expect(blockSpan({ title: "t", items: [kv(6)] })).toBe("full");
  });

  it("charts and compact summaries share a row", () => {
    expect(blockSpan({ title: "t", items: [table(3)] })).toBe("cell");
    expect(blockSpan({ title: "t", items: [kv(4)] })).toBe("cell");
    expect(
      blockSpan({
        title: "t",
        items: [
          {
            type: "chart",
            chart: {
              xLabel: "x",
              yLabel: "y",
              data: { type: "bar", categories: [], values: [] },
            },
          },
        ],
      }),
    ).toBe("cell");
  });

  it("a block with no fragment takes a cell", () => {
    expect(blockSpan(undefined)).toBe("cell");
  });
});

describe("layoutSpans", () => {
  // Shapes that individually span a cell / the full row.
  const cell = (): Fragment => ({ title: "c", items: [] });
  const full = (): Fragment => ({
    title: "f",
    items: [
      {
        type: "table",
        table: {
          columns: Array.from({ length: 5 }, (_, i) => ({
            name: `c${i}`,
            kind: "number",
          })),
          rows: [],
        },
      },
    ],
  });

  it("a lone cell before a full row takes the whole row itself", () => {
    // The screenshot case: Subcatchment Peaks (cell) stranded beside a
    // hole because Runoff Summary (full) opened its own row.
    expect(layoutSpans([cell(), full()])).toEqual(["full", "full"]);
  });

  it("even runs pair up untouched", () => {
    expect(layoutSpans([cell(), cell(), full()])).toEqual([
      "cell",
      "cell",
      "full",
    ]);
  });

  it("an odd run promotes only its straggler", () => {
    expect(layoutSpans([cell(), cell(), cell(), full()])).toEqual([
      "cell",
      "cell",
      "full",
      "full",
    ]);
    // A trailing odd run promotes too — the last row must also be whole.
    expect(layoutSpans([full(), cell()])).toEqual(["full", "full"]);
  });

  it("blocks without fragments count as cells in the pairing", () => {
    expect(layoutSpans([undefined, undefined])).toEqual(["cell", "cell"]);
    expect(layoutSpans([undefined])).toEqual(["full"]);
  });
});

const block = (id: string, category: string): AnalysisBlock => ({
  id,
  title: id,
  category,
  status: "ok",
});

describe("categoriesOf", () => {
  it("keeps first-appearance order, which is catalog order", () => {
    expect(
      categoriesOf([
        block("a", "Summary"),
        block("b", "Compliance"),
        block("c", "Summary"),
        block("d", "Assets"),
      ]),
    ).toEqual(["Summary", "Compliance", "Assets"]);
  });

  it("no blocks means no tabs", () => {
    expect(categoriesOf([])).toEqual([]);
  });
});

describe("activeCategoryOf", () => {
  const tabs = ["Summary", "Compliance"];

  it("honours the user's pick while it exists", () => {
    expect(activeCategoryOf("Compliance", tabs)).toBe("Compliance");
  });

  it("falls back to the first tab when the pick vanishes or is unset", () => {
    // A re-run can drop a category (quality turned off); the page must
    // not stay on a ghost tab.
    expect(activeCategoryOf("Quality", tabs)).toBe("Summary");
    expect(activeCategoryOf(null, tabs)).toBe("Summary");
  });

  it("no tabs means no active category", () => {
    expect(activeCategoryOf("Summary", [])).toBeNull();
  });
});
