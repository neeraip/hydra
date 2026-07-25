import { useCallback, useMemo } from "react";
import { useAppState } from "../../../AppContext";
import { useCanvasSelection } from "../../../canvas/selection-context";
import type { ResultAnalytics } from "../../../hooks";
import { formatQty, useUnitSystem } from "../../../units";
import { NoDataCard } from "./charts";

/**
 * Ranked "problem junctions" — the lowest worst-case pressures in the run.
 * Clicking a row jumps to the Canvas and selects + zooms to that node, closing
 * the analyse → locate loop that engineers do constantly.
 */
export function WorstNodesPanel({
  analytics,
  minPressureM,
}: {
  analytics: ResultAnalytics | null;
  /** Compliance criterion (SI metres); rows below it are flagged. */
  minPressureM: number;
}) {
  const sys = useUnitSystem();
  const { setProjectView } = useAppState();
  const { selectNode, zoomToNode } = useCanvasSelection();

  const rows = useMemo(
    () => (analytics ? analytics.worstNodes : null),
    [analytics],
  );

  const locate = useCallback(
    (id: string) => {
      setProjectView("canvas");
      selectNode(id);
      // Let the canvas view activate and its map initialise before flying.
      window.setTimeout(() => zoomToNode(id), 220);
    },
    [setProjectView, selectNode, zoomToNode],
  );

  const thStyle: React.CSSProperties = {
    fontSize: 11,
    fontWeight: 500,
    color: "var(--text-tertiary)",
    textAlign: "left",
    padding: "4px 8px",
    borderBottom: "1px solid var(--border)",
    whiteSpace: "nowrap",
  };
  const tdStyle: React.CSSProperties = {
    padding: "7px 8px",
    fontSize: 12,
    borderBottom: "1px solid var(--border)",
    fontFamily: "var(--font-mono)",
  };

  if (!rows || rows.length === 0) {
    return (
      <div className="insights-card">
        <div
          style={{
            marginBottom: 8,
            fontSize: 13,
            fontWeight: 500,
            color: "var(--text-primary)",
          }}
        >
          Lowest-Pressure Junctions
        </div>
        <NoDataCard message="Run a simulation to see the lowest-pressure junctions." />
      </div>
    );
  }

  return (
    <div className="insights-card">
      <div style={{ marginBottom: 12 }}>
        <div
          style={{
            fontSize: 13,
            fontWeight: 500,
            color: "var(--text-primary)",
          }}
        >
          Lowest-Pressure Junctions
        </div>
        <div
          style={{ fontSize: 11, color: "var(--text-tertiary)", marginTop: 2 }}
        >
          Worst-case pressure over the run · click a row to locate on the map
        </div>
      </div>
      <table style={{ width: "100%", borderCollapse: "collapse" }}>
        <thead>
          <tr>
            <th style={{ ...thStyle, width: 28 }}>#</th>
            <th style={thStyle}>Junction</th>
            <th style={{ ...thStyle, textAlign: "right" }}>Min pressure</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row, i) => {
            const below = row.minPressureM < minPressureM;
            return (
              <tr
                key={row.id}
                onClick={() => locate(row.id)}
                style={{
                  cursor: "pointer",
                  background:
                    i % 2 === 0 ? "transparent" : "rgba(255,255,255,0.02)",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.background = "var(--nav-hover)";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.background =
                    i % 2 === 0 ? "transparent" : "rgba(255,255,255,0.02)";
                }}
                title="Show on the map"
              >
                <td style={{ ...tdStyle, color: "var(--text-tertiary)" }}>
                  {i + 1}
                </td>
                <td
                  style={{
                    ...tdStyle,
                    color: "var(--text-primary)",
                    fontWeight: 500,
                  }}
                >
                  {row.id}
                </td>
                <td
                  style={{
                    ...tdStyle,
                    textAlign: "right",
                    color: below
                      ? "var(--status-error)"
                      : "var(--text-secondary)",
                    fontWeight: below ? 600 : 400,
                  }}
                >
                  {formatQty(row.minPressureM, "pressure", sys, 1)}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
