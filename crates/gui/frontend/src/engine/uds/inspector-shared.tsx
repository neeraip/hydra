// ── Shared pieces of the drainage inspector bodies ────────────────────────────
// The uds node/link bodies mirror the wds body's section structure —
// Properties (model attributes), Connected elements, Results (cards) —
// with engine-authored content: attribute rows come from the §4 schema via
// `get_element_details`, result values from the §6 catalog payload.

import { useEffect, useMemo, useState } from "react";
import { useActiveProject, useAppState, useSimulation } from "../../AppContext";
import { useCurrentPeriod } from "../../canvas/period-context";
import {
  BigValue,
  PropRow,
  SecondaryCell,
} from "../../components/panels/ElementInspector/primitives";
import { SectionLabel } from "../../components/ui/SectionLabel";
import {
  ACCENT,
  type ElementAttribute,
  type ElementSeries,
  formatElementAttribute,
  type GenericVariable,
  getElementDetails,
  getElementSeries,
  useNetworkData,
} from "../../hooks";
import { Sparkline } from "../../pages/project/AnalysisPanel/charts";
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

/** How many series charts show before the "more fields" toggle — mirrors
 * the wds card's primary/extra split. */
const PRIMARY_SERIES_FIELDS = 3;

/**
 * Per-element time-series charts, mirroring the wds TimeSeriesCard: one
 * sparkline per catalog variable (labels and units engine-authored, values
 * as served — the file's own unit system), the current scrub period as a
 * marker, primary fields up front and the rest behind a toggle.
 * Steady-state runs (≤1 period) render nothing.
 */
export function GenericTimeSeriesCard({
  kind,
  elementId,
}: {
  kind: "node" | "link";
  elementId: string;
}) {
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();
  const { resultMeta, resultGeneration } = useSimulation();
  const { nodes, links } = useNetworkData();
  const currentPeriod = useCurrentPeriod();

  const [series, setSeries] = useState<ElementSeries | null>(null);
  const [loading, setLoading] = useState(false);
  const [showAll, setShowAll] = useState(false);

  const projectId = project?.id ?? null;
  const periods = resultMeta?.times.length ?? 0;
  const variables: GenericVariable[] =
    (kind === "node"
      ? resultMeta?.generic?.pointVars
      : resultMeta?.generic?.polylineVars) ?? [];

  // The backend addresses series by snapshot index, the same order the
  // NetworkDataContext arrays carry.
  const index = useMemo(() => {
    const arr: Array<{ id: string }> = kind === "node" ? nodes : links;
    return arr.findIndex((el) => el.id === elementId);
  }, [kind, nodes, links, elementId]);

  const enabled =
    projectId != null && periods > 1 && index >= 0 && variables.length > 0;

  // biome-ignore lint/correctness/useExhaustiveDependencies: resultGeneration is an intentional refetch trigger — a completed run must refresh the charts even though the effect body never reads it.
  useEffect(() => {
    if (!enabled || projectId == null) {
      setSeries(null);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setSeries(null);
    setLoading(true);
    getElementSeries(projectId, activeScenarioId ?? null, kind, index).then(
      (s) => {
        if (cancelled) return;
        setSeries(s);
        setLoading(false);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [enabled, projectId, activeScenarioId, resultGeneration, kind, index]);

  if (!enabled) return null;

  // Charts in catalog order; labels/units joined from the generic meta.
  const charts = (series?.fields ?? [])
    .map((field) => ({
      field,
      variable: variables.find((v) => v.id === field.name),
    }))
    .filter((c) => c.variable != null);
  const shown = showAll ? charts : charts.slice(0, PRIMARY_SERIES_FIELDS);
  const extraCount =
    charts.length - Math.min(charts.length, PRIMARY_SERIES_FIELDS);

  if (!loading && (series == null || series.times.length < 2)) return null;

  return (
    <>
      <SectionLabel>Time series</SectionLabel>
      <div
        style={{
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
          borderRadius: 8,
          padding: "12px 12px 10px",
          marginBottom: 14,
          display: "flex",
          flexDirection: "column",
          gap: 10,
        }}
      >
        {loading ? (
          <span
            style={{
              fontSize: "var(--text-md)",
              color: "var(--text-secondary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            Loading time series…
          </span>
        ) : (
          <>
            {shown.map(({ field, variable }) => {
              let min = Number.POSITIVE_INFINITY;
              let max = Number.NEGATIVE_INFINITY;
              for (const v of field.values) {
                if (!Number.isFinite(v)) continue;
                if (v < min) min = v;
                if (v > max) max = v;
              }
              if (min > max) {
                min = 0;
                max = 0;
              }
              const marker =
                currentPeriod == null || field.values.length === 0
                  ? null
                  : Math.max(
                      0,
                      Math.min(currentPeriod, field.values.length - 1),
                    );
              return (
                <div key={field.name}>
                  <div
                    style={{
                      fontSize: "var(--text-xs)",
                      color: "var(--text-tertiary)",
                      textTransform: "uppercase",
                      letterSpacing: "0.06em",
                      marginBottom: 3,
                    }}
                  >
                    {variable?.label}
                    {variable?.unit ? ` (${variable.unit})` : ""}
                  </div>
                  <Sparkline
                    values={field.values}
                    min={min}
                    max={max}
                    stroke="var(--accent)"
                    times={series?.times}
                    markerIndex={marker}
                    unit={variable?.unit}
                    decimals={2}
                    height={40}
                  />
                </div>
              );
            })}
            {extraCount > 0 && (
              <button
                type="button"
                onClick={() => setShowAll((v) => !v)}
                style={{
                  alignSelf: "flex-start",
                  background: "transparent",
                  border: "none",
                  padding: 0,
                  cursor: "pointer",
                  fontSize: "var(--text-sm)",
                  color: "var(--text-secondary)",
                  fontFamily: "var(--font-ui)",
                  textDecoration: "underline",
                  textUnderlineOffset: 2,
                }}
              >
                {showAll
                  ? "Show fewer fields"
                  : `Show ${extraCount} more field${extraCount === 1 ? "" : "s"}`}
              </button>
            )}
          </>
        )}
      </div>
    </>
  );
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
