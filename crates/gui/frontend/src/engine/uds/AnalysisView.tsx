/** The uds implementation of the Results view: the engine's report blocks
 * rendered live as analysis panels — the analysis-as-blocks convergence,
 * arriving engine-by-engine. Fragments are engine-authored neutral data
 * (hydra-common §3.3); this component renders shapes, never meanings. */

import { useEffect, useState } from "react";
import { useActiveProject, useAppState, useSimulation } from "../../AppContext";
import { tryInvokeOr } from "../../hooks/ipc";
import { useUnitSystem } from "../../units";

// ── Fragment wire shapes (hydra-common §3.3, serde camelCase) ────────────────

type Value =
  | { type: "number"; value: number; unit?: string }
  | { type: "integer"; value: number }
  | { type: "boolean"; value: boolean }
  | { type: "text"; value: string }
  | { type: "timestamp"; value: string }
  | { type: "absent" };

interface TableShape {
  columns: Array<{ name: string; unit?: string; kind: string }>;
  rows: Value[][];
}

type FragmentItem =
  | { type: "keyValues"; entries: Array<{ label: string; value: Value }> }
  | { type: "table"; table: TableShape }
  | { type: "note"; text: string }
  | { type: "chart"; chart: unknown };

interface Fragment {
  title: string;
  items: FragmentItem[];
}

interface AnalysisBlock {
  id: string;
  title: string;
  status: "ok" | "unavailable" | "failed";
  reason?: string;
  fragment?: Fragment;
}

function formatValue(v: Value): string {
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

function Panel({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div
      style={{
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius: 6,
        padding: "12px 14px",
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
        {title}
      </div>
      {children}
    </div>
  );
}

function FragmentBody({ fragment }: { fragment: Fragment }) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      {fragment.items.map((item, i) => {
        const key = `${fragment.title}-${i}`;
        if (item.type === "keyValues") {
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
        }
        if (item.type === "table") {
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
        }
        if (item.type === "note") {
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
        }
        return null;
      })}
    </div>
  );
}

export function UdsAnalysisView() {
  const { activeProjectId, activeScenarioId } = useAppState();
  const { project } = useActiveProject();
  const { resultGeneration } = useSimulation();
  // The reader's resolved display system: tagged block values arrive from
  // the backend already re-expressed in it, so this view renders what it
  // is given and flipping the preference refetches.
  const unitSystem = useUnitSystem();
  const [blocks, setBlocks] = useState<AnalysisBlock[] | null>(null);

  // resultGeneration is a re-run token: a completed run bumps it so the
  // panels refresh with the new results.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-run token, see above
  useEffect(() => {
    if (!activeProjectId) return;
    let cancelled = false;
    setBlocks(null);
    void tryInvokeOr<AnalysisBlock[]>(
      "get_analysis_blocks",
      { projectId: activeProjectId, scenarioId: activeScenarioId, unitSystem },
      [],
    ).then((b) => {
      if (!cancelled) setBlocks(b);
    });
    return () => {
      cancelled = true;
    };
  }, [activeProjectId, activeScenarioId, resultGeneration, unitSystem]);

  if (!project) return null;
  if (blocks === null) {
    return (
      <div style={{ padding: 18, color: "var(--text-tertiary)" }}>Loading…</div>
    );
  }
  if (blocks.length === 0) {
    return (
      <div
        style={{
          padding: 18,
          color: "var(--text-tertiary)",
          fontSize: "var(--text-md)",
        }}
      >
        Run a simulation to see results here.
      </div>
    );
  }
  return (
    <div
      style={{
        flex: 1,
        overflowY: "auto",
        padding: 18,
        display: "flex",
        flexDirection: "column",
        gap: 12,
      }}
    >
      {blocks.map((b) => (
        <Panel key={b.id} title={b.title}>
          {b.fragment ? (
            <FragmentBody fragment={b.fragment} />
          ) : (
            <div
              style={{
                fontSize: "var(--text-md)",
                color: "var(--text-tertiary)",
              }}
            >
              {b.reason ?? "Unavailable for this run."}
            </div>
          )}
        </Panel>
      ))}
    </div>
  );
}
