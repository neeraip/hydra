import { useMemo } from "react";
import type { ResultAnalytics } from "../../../hooks";
import { NoDataCard } from "./charts";

export function PipeCriticality({
  analytics,
}: {
  analytics: ResultAnalytics | null;
}) {
  const rows = useMemo(() => {
    if (!analytics) return null;
    return analytics.topPipes.map((p) => ({
      id: p.id,
      diameter: p.diameterMm > 0 ? Math.round(p.diameterMm) : 0,
      velocity: p.maxVelocityMs,
      segment: `${p.fromId} → ${p.toId}`,
      score: Math.min(100, Math.round(p.maxVelocityMs * 70)),
    }));
  }, [analytics]);

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

  if (!rows) {
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
          Pipe Criticality
        </div>
        <NoDataCard message="Run a simulation to see pipe criticality rankings." />
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
          Pipe Criticality
        </div>
        <div
          style={{ fontSize: 11, color: "var(--text-tertiary)", marginTop: 2 }}
        >
          Top 5 by peak velocity
        </div>
      </div>
      <table style={{ width: "100%", borderCollapse: "collapse" }}>
        <thead>
          <tr>
            <th style={thStyle}>ID</th>
            <th style={thStyle}>Segment</th>
            <th style={{ ...thStyle, textAlign: "right" }}>Ø (mm)</th>
            <th style={{ ...thStyle, textAlign: "right" }}>Velocity</th>
            <th style={{ ...thStyle, textAlign: "right", width: 140 }}>
              Score
            </th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row, i) => (
            <tr
              key={row.id}
              style={{
                background:
                  i % 2 === 0 ? "transparent" : "rgba(255,255,255,0.02)",
              }}
            >
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
                  fontFamily: "var(--font-ui)",
                  color: "var(--text-secondary)",
                }}
              >
                {row.segment}
              </td>
              <td style={{ ...tdStyle, textAlign: "right" }}>{row.diameter}</td>
              <td
                style={{
                  ...tdStyle,
                  textAlign: "right",
                  color:
                    row.velocity > 1.0
                      ? "var(--status-warning)"
                      : row.velocity > 0.6
                        ? "var(--text-primary)"
                        : "var(--text-secondary)",
                }}
              >
                {row.velocity.toFixed(2)} m/s
              </td>
              <td style={{ ...tdStyle, textAlign: "right" }}>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "flex-end",
                    gap: 8,
                  }}
                >
                  <span style={{ color: "var(--text-primary)" }}>
                    {row.score}
                  </span>
                  <div
                    style={{
                      width: 60,
                      height: 4,
                      background: "var(--border)",
                      borderRadius: 2,
                      overflow: "hidden",
                    }}
                  >
                    <div
                      style={{
                        width: `${row.score}%`,
                        height: "100%",
                        background: "var(--accent)",
                        borderRadius: 2,
                      }}
                    />
                  </div>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
