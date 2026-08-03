// ── Shared pieces of the drainage inspector bodies ────────────────────────────
// The uds node/link bodies mirror the wds body's section structure —
// Properties (model attributes), Connected elements, Results (cards) —
// with engine-authored content: attribute rows come from the §4 schema via
// `get_element_details`, result values from the §6 catalog payload.

import { useEffect, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import {
  BigValue,
  PropRow,
  SecondaryCell,
} from "../../components/panels/ElementInspector/primitives";
import { SectionLabel } from "../../components/ui/SectionLabel";
import {
  ACCENT,
  type ElementAttribute,
  formatElementAttribute,
  getElementDetails,
} from "../../hooks";
import { useUnitSystem } from "../../units";
import type { GenericElementValue } from "../registry";

/** Fetch the engine-described attribute rows for one element. */
export function useElementDetails(
  elementId: string,
): ElementAttribute[] | null {
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();
  const [rows, setRows] = useState<ElementAttribute[] | null>(null);
  useEffect(() => {
    if (!project?.id) return;
    let cancelled = false;
    getElementDetails(project.id, activeScenarioId, elementId).then((r) => {
      if (!cancelled) setRows(r);
    });
    return () => {
      cancelled = true;
    };
  }, [project?.id, activeScenarioId, elementId]);
  return rows;
}

/** Properties section: §4 schema rows in the wds table presentation. */
export function PropertiesSection({
  rows,
}: {
  rows: ElementAttribute[] | null;
}) {
  const sys = useUnitSystem();
  if (!rows || rows.length === 0) return null;
  return (
    <>
      <SectionLabel>Properties</SectionLabel>
      <table
        style={{ width: "100%", borderCollapse: "collapse", marginBottom: 14 }}
      >
        <tbody>
          {rows.map((r) => (
            <PropRow
              key={r.label}
              label={r.label}
              value={formatElementAttribute(r, sys)}
            />
          ))}
        </tbody>
      </table>
    </>
  );
}

/** Magnitude-aware value text with the engine-authored unit label. */
function formatResultValue(v: GenericElementValue): string {
  if (v.value == null || !Number.isFinite(v.value)) return "—";
  const a = Math.abs(v.value);
  const text =
    a >= 1000
      ? Math.round(v.value).toLocaleString()
      : a >= 10
        ? v.value.toFixed(1)
        : v.value.toFixed(2);
  return v.unit ? `${text} ${v.unit}` : text;
}

/**
 * Results section in the wds card presentation: the primary variable (the
 * legend's selection) as the big value, every other catalog variable as a
 * secondary cell, and the shared empty state before a run.
 */
export function GenericResultsCards({
  results,
}: {
  results?: GenericElementValue[] | null;
}) {
  const rows = results ?? [];
  return (
    <>
      <SectionLabel>Results</SectionLabel>
      {rows.length === 0 ? (
        <div
          style={{
            background: "var(--bg-card)",
            border: "1px solid var(--border)",
            borderRadius: 8,
            padding: "16px 12px",
            marginBottom: 14,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <span
            style={{
              fontSize: "var(--text-md)",
              color: "var(--text-secondary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            Run a simulation to see results
          </span>
        </div>
      ) : (
        (() => {
          const primary = rows.find((r) => r.primary) ?? rows[0];
          const secondaries = rows.filter((r) => r !== primary);
          return (
            <div
              style={{
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                borderRadius: 8,
                padding: "14px 12px 12px",
                marginBottom: 14,
                display: "flex",
                flexDirection: "column",
                gap: 12,
              }}
            >
              <BigValue
                label={primary.label}
                value={formatResultValue(primary)}
                color={ACCENT}
              />
              {secondaries.length > 0 && (
                <div
                  style={{
                    display: "grid",
                    gridTemplateColumns: "1fr 1fr",
                    gap: 6,
                  }}
                >
                  {secondaries.map((s) => (
                    <SecondaryCell
                      key={s.id}
                      label={s.label}
                      value={formatResultValue(s)}
                    />
                  ))}
                </div>
              )}
            </div>
          );
        })()
      )}
    </>
  );
}
