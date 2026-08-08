/** Renders one report-block fragment as an analysis panel — the shared
 * presentation of the analysis-as-blocks convergence. Engine-neutral by
 * construction: fragments carry no engine vocabulary (hydra-common §3.3),
 * so one renderer serves every engine without a single engine switch. */

import {
  HorizontalBarChart,
  Sparkline,
} from "../../pages/project/AnalysisPanel/charts";
import {
  type AnalysisBlock,
  type ChartShape,
  chartBars,
  type Fragment,
  formatValue,
  lineSeriesView,
} from "./fragments";

/** Distinct, colorblind-safe series strokes, cycled by series index. */
const SERIES_STROKES = [
  "var(--chart-1, #4aa3ff)",
  "var(--chart-2, #58c98a)",
  "var(--chart-3, #e0b155)",
  "var(--chart-4, #b48ef0)",
  "var(--chart-5, #ef8f6b)",
  "var(--chart-6, #62c9c3)",
  "var(--chart-7, #e07ab8)",
  "var(--chart-8, #9aa76a)",
];

export function BlockPanel({ block }: { block: AnalysisBlock }) {
  return (
    <div
      style={{
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius: 6,
        padding: "12px 14px",
        // Fill the grid row: cards sharing a row share its height, so a
        // short card ("The network has no tanks.") sits in a full-height
        // card beside its neighbour instead of leaving a hole below.
        height: "100%",
        boxSizing: "border-box",
      }}
    >
      <div
        style={{
          fontSize: "var(--text-sm)",
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: "0.05em",
          marginBottom: 10,
        }}
      >
        {block.title}
      </div>
      {block.fragment ? (
        <FragmentBody fragment={block.fragment} />
      ) : (
        <div
          style={{ fontSize: "var(--text-md)", color: "var(--text-tertiary)" }}
        >
          {block.reason ?? "Unavailable for this run."}
        </div>
      )}
    </div>
  );
}

export function FragmentBody({ fragment }: { fragment: Fragment }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      {fragment.items.map((item, i) => {
        const key = `${fragment.title}-${i}`;
        switch (item.type) {
          case "keyValues":
            return (
              <div
                key={key}
                style={{
                  display: "grid",
                  gridTemplateColumns: "repeat(auto-fill, minmax(190px, 1fr))",
                  gap: "6px 16px",
                }}
              >
                {item.entries.map((e) => (
                  <div
                    key={e.label}
                    style={{
                      display: "flex",
                      justifyContent: "space-between",
                      gap: 8,
                      fontSize: "var(--text-md)",
                    }}
                  >
                    <span style={{ color: "var(--text-tertiary)" }}>
                      {e.label}
                    </span>
                    <span style={{ color: "var(--text-primary)" }}>
                      {formatValue(e.value)}
                    </span>
                  </div>
                ))}
              </div>
            );
          case "table":
            return (
              <div key={key} style={{ overflowX: "auto" }}>
                <table
                  style={{
                    borderCollapse: "collapse",
                    width: "100%",
                    fontSize: "var(--text-md)",
                  }}
                >
                  <thead>
                    <tr>
                      {item.table.columns.map((c) => (
                        <th
                          key={c.name}
                          style={{
                            textAlign: "left",
                            padding: "4px 10px 4px 0",
                            color: "var(--text-tertiary)",
                            fontWeight: 500,
                            fontSize: "var(--text-sm)",
                            borderBottom: "1px solid var(--border)",
                            whiteSpace: "nowrap",
                          }}
                        >
                          {c.name}
                          {c.unit ? ` (${c.unit})` : ""}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {item.table.rows.map((row, ri) => (
                      // Row order is stable block output — index keys are safe.
                      // biome-ignore lint/suspicious/noArrayIndexKey: stable order
                      <tr key={ri}>
                        {row.map((cell, ci) => (
                          <td
                            key={item.table.columns[ci]?.name ?? String(ci)}
                            style={{
                              padding: "4px 10px 4px 0",
                              color: "var(--text-primary)",
                              borderBottom: "1px solid var(--border)",
                              whiteSpace: "nowrap",
                            }}
                          >
                            {formatValue(cell)}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            );
          case "note":
            return (
              <div
                key={key}
                style={{
                  fontSize: "var(--text-sm)",
                  color: "var(--text-tertiary)",
                  lineHeight: 1.5,
                }}
              >
                {item.text}
              </div>
            );
          case "chart":
            return <FragmentChart key={key} chart={item.chart} />;
          default:
            return null;
        }
      })}
    </div>
  );
}

/** A fragment chart, drawn with the same primitives the rest of the app
 * charts with. Bar charts carry their band labels as categories (the
 * engine authored them); line charts draw one sparkline per series, each
 * scaled to its own range — a fragment's series (tank heads at different
 * elevations) can sit in disjoint bands, and one shared scale would
 * flatten all but the widest. */
function FragmentChart({ chart }: { chart: ChartShape }) {
  // Constrained: these are scalable SVGs, and at full panel width a
  // 350-unit viewBox renders at 5× design size — bars like billboards.
  const CHART_MAX_WIDTH = 560;
  if (chart.data.type === "bar") {
    const { bars, max } = chartBars(chart);
    return (
      <div style={{ maxWidth: CHART_MAX_WIDTH }}>
        <HorizontalBarChart
          bars={bars.map((b) => ({ ...b, fill: SERIES_STROKES[0] }))}
          maxCount={max}
        />
        <div
          style={{
            marginTop: 4,
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
          }}
        >
          {chart.yLabel} by {chart.xLabel.toLowerCase()}
          {chart.xUnit ? ` (${chart.xUnit})` : ""}
        </div>
      </div>
    );
  }
  const series = lineSeriesView(chart);
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 8,
        maxWidth: CHART_MAX_WIDTH,
      }}
    >
      {series.map((s, i) => (
        <div key={s.name}>
          <div
            style={{
              fontSize: "var(--text-sm)",
              color: "var(--text-tertiary)",
              marginBottom: 2,
            }}
          >
            {s.name}
          </div>
          <Sparkline
            values={s.values}
            min={s.min}
            max={s.max}
            stroke={SERIES_STROKES[i % SERIES_STROKES.length]}
            times={s.times}
            unit={chart.yUnit}
            decimals={1}
          />
        </div>
      ))}
    </div>
  );
}
