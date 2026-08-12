// ── Shared pieces of the drainage inspector bodies ────────────────────────────
// The uds node/link bodies mirror the wds body's section structure —
// Properties (model attributes), Connected elements, Results (cards) —
// with engine-authored content: attribute rows come from the §4 schema via
// `get_element_details`, result values from the §6 catalog payload.

import { useCallback, useEffect, useMemo, useState } from "react";
import { useActiveProject, useAppState, useSimulation } from "../../AppContext";
import { useCurrentPeriod } from "../../canvas/period-context";
import {
  BigValue,
  PropRow,
  SecondaryCell,
} from "../../components/panels/ElementInspector/primitives";
import {
  elementSeriesCacheKey,
  LruCache,
} from "../../components/panels/ElementInspector/seriesCache";
import { EditableNumber } from "../../components/ui/EditableNumber";
import { SectionLabel } from "../../components/ui/SectionLabel";
import {
  ACCENT,
  type ElementAttribute,
  type ElementAttributeInfo,
  type ElementSeries,
  type ElementSeriesKind,
  editableNumberOf,
  formatElementAttribute,
  formatGenericValue,
  type GenericVariable,
  genericToDisplay,
  genericUnitLabel,
  getElementDetails,
  getElementSeries,
  useElementAttributes,
  useNetworkData,
} from "../../hooks";
import { useElementAttributeWrite } from "../../hooks/useAttributeWrite";
import { Sparkline } from "../../pages/project/AnalysisPanel/charts";
import { useUnitSystem } from "../../units";
import type { GenericElementValue } from "../registry";
import { seriesIndex, seriesVariables } from "./seriesAddressing";

/** Fetch the engine-described attribute rows for one element. */
/** Rows already fetched, so re-selecting an element does not go blank
 * while the same answer is fetched again. */
const detailCache = new Map<string, ElementAttribute[]>();

/**
 * A element's §4.4 property rows, and how much space to leave for them.
 *
 * Properties arrive over IPC, and the node, link and region bodies are
 * separate components — so selecting a junction after a catchment mounts a
 * fresh body whose rows are null for one round trip. Rendering nothing in
 * that gap collapsed the section, then restored it, shoving everything
 * below it down the panel.
 *
 * The schema is the answer to that: a kind's properties are declared, so
 * their names are known before any element is fetched. The section draws
 * its real rows immediately and fills the values in when they land.
 */
export function useElementDetails(
  elementId: string,
  kind?: string,
): {
  rows: ElementAttribute[] | null;
  schema: ElementAttributeInfo[];
  elementId: string;
  onEdited: () => void;
} {
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();
  const schema = useElementAttributes(project?.engine, kind);
  const key = `${project?.id ?? ""}\u0000${activeScenarioId ?? ""}\u0000${elementId}`;
  const [rows, setRows] = useState<ElementAttribute[] | null>(
    () => detailCache.get(key) ?? null,
  );
  // After a write, fetch what the model now holds and replace the cached
  // answer. Refetching directly rather than through a counter that only
  // exists to re-run the effect: the cache is there so re-selecting an
  // element is instant, not so an edit is invisible.
  const onEdited = useCallback(() => {
    if (!project?.id) return;
    getElementDetails(project.id, activeScenarioId, elementId).then((r) => {
      if (r) detailCache.set(key, r);
      setRows(r);
    });
  }, [project?.id, activeScenarioId, elementId, key]);
  useEffect(() => {
    if (!project?.id) return;
    const cached = detailCache.get(key);
    if (cached) {
      setRows(cached);
      return;
    }
    setRows(null);
    let cancelled = false;
    getElementDetails(project.id, activeScenarioId, elementId).then((r) => {
      if (r) detailCache.set(key, r);
      if (!cancelled) setRows(r);
    });
    return () => {
      cancelled = true;
    };
  }, [project?.id, activeScenarioId, elementId, key]);
  return { rows, schema, elementId, onEdited };
}

/** Properties section: §4 schema rows in the wds table presentation. */
export function PropertiesSection({
  rows,
  schema = [],
  elementId,
  onEdited,
}: {
  rows: ElementAttribute[] | null;
  /** The kind's declared properties, drawn while the values load. */
  schema?: ElementAttributeInfo[];
  /** The element these rows belong to. Absent = the section reads only,
   * which is what a caller with no element to address should get. */
  elementId?: string;
  /** Called after a successful write, so the caller can refetch. */
  onEdited?: () => void;
}) {
  const sys = useUnitSystem();
  // Nothing known yet, but this kind has been seen before: hold the height
  // rather than collapsing and shoving the rest of the panel about.
  // Labels are declared, values are fetched. Draw what is known and leave
  // the values blank for the moment rather than drawing nothing at all.
  if (!rows && schema.length > 0) {
    return (
      <>
        <SectionLabel>Properties</SectionLabel>
        <table
          style={{
            width: "100%",
            borderCollapse: "collapse",
            marginBottom: 14,
          }}
        >
          <tbody>
            {schema.map((a) => (
              <PropRow key={a.key} label={a.label} value="—" />
            ))}
          </tbody>
        </table>
      </>
    );
  }
  if (!rows || rows.length === 0) return null;
  return (
    <>
      <SectionLabel>Properties</SectionLabel>
      <table
        style={{ width: "100%", borderCollapse: "collapse", marginBottom: 14 }}
      >
        <tbody>
          {rows.map((r) =>
            elementId ? (
              <AttrRow
                key={r.key}
                attr={r}
                sys={sys}
                elementId={elementId}
                onEdited={onEdited}
              />
            ) : (
              <PropRow
                key={r.key}
                label={r.label}
                value={formatElementAttribute(r, sys)}
              />
            ),
          )}
        </tbody>
      </table>
    </>
  );
}

/**
 * One Properties row, editable where the attribute allows it.
 *
 * Both cases live in one component so the decision is made once, from
 * `editableNumberOf` — the same rule the Editor's tables apply to a
 * cell. Splitting it across the caller's ternary was how the two
 * surfaces would come to disagree about which rows take an input.
 *
 * The field itself is the app's shared one; what this row adds is where
 * the number sits — in `PropRow`'s grid, so an editable row lines up
 * with the read-only rows above and below it rather than reading as a
 * second table — and the unit beside it, labelling rather than
 * participating.
 */
function AttrRow({
  attr,
  sys,
  elementId,
  onEdited,
}: {
  attr: ElementAttribute;
  sys: "si" | "us";
  elementId: string;
  onEdited?: () => void;
}) {
  const write = useElementAttributeWrite();
  const q = attr.quantity;
  const value = editableNumberOf(attr.editable, attr.number);
  if (value == null) {
    return (
      <PropRow label={attr.label} value={formatElementAttribute(attr, sys)} />
    );
  }

  return (
    <tr>
      <td
        style={{
          fontSize: "var(--text-md)",
          color: "var(--text-tertiary)",
          padding: "4px 0",
          width: "45%",
        }}
      >
        {attr.label}
      </td>
      <td
        style={{
          fontSize: "var(--text-md)",
          padding: "4px 0",
          fontFamily: "var(--font-mono)",
          display: "flex",
          alignItems: "center",
          gap: 6,
        }}
      >
        <EditableNumber
          value={value}
          quantity={q}
          sys={sys}
          label={attr.label}
          onCommit={(next) =>
            write(elementId, attr.key, next).then(() => onEdited?.())
          }
        />
        {q && (
          <span style={{ color: "var(--text-tertiary)" }}>
            {sys === "us" ? q.usLabel : q.siLabel}
          </span>
        )}
      </td>
    </tr>
  );
}

/** How many series charts show before the "more fields" toggle — mirrors
 * the wds card's primary/extra split. */
const PRIMARY_SERIES_FIELDS = 3;

/** Module-level cache, like the wds card's: survives element re-selection
 * and inspector remounts; entries are keyed per run via resultGeneration. */
const seriesCache = new LruCache<ElementSeries | null>(24);

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
  kind: ElementSeriesKind;
  elementId: string;
}) {
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();
  const { resultMeta, resultGeneration } = useSimulation();
  const { nodes, links, regions } = useNetworkData();
  const currentPeriod = useCurrentPeriod();
  const sys = useUnitSystem();

  const [series, setSeries] = useState<ElementSeries | null>(null);
  const [loading, setLoading] = useState(false);
  const [showAll, setShowAll] = useState(false);

  const projectId = project?.id ?? null;
  const periods = resultMeta?.times.length ?? 0;
  const variables: GenericVariable[] = seriesVariables(
    resultMeta?.generic,
    kind,
  );

  // The backend addresses series by snapshot index, the same order the
  // NetworkDataContext arrays carry.
  const index = useMemo(
    () => seriesIndex({ nodes, links, regions }, kind, elementId),
    [kind, nodes, links, regions, elementId],
  );

  const enabled =
    projectId != null && periods > 1 && index >= 0 && variables.length > 0;

  useEffect(() => {
    if (!enabled || projectId == null) {
      setSeries(null);
      setLoading(false);
      return;
    }
    const key = elementSeriesCacheKey({
      projectId,
      scenarioId: activeScenarioId ?? null,
      resultGeneration,
      kind,
      elementId,
    });
    const cached = seriesCache.get(key);
    if (cached !== undefined) {
      setSeries(cached);
      setLoading(false);
      return;
    }
    let cancelled = false;
    setSeries(null);
    setLoading(true);
    getElementSeries(projectId, activeScenarioId ?? null, kind, index).then(
      (s) => {
        if (cancelled) return;
        seriesCache.set(key, s);
        setSeries(s);
        setLoading(false);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [
    enabled,
    projectId,
    activeScenarioId,
    resultGeneration,
    kind,
    index,
    elementId,
  ]);

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
              // Convert at the render boundary — the fetched series stays SI.
              const values = field.values.map((v) =>
                genericToDisplay(v, variable?.quantity, sys),
              );
              let min = Number.POSITIVE_INFINITY;
              let max = Number.NEGATIVE_INFINITY;
              for (const v of values) {
                if (!Number.isFinite(v)) continue;
                if (v < min) min = v;
                if (v > max) max = v;
              }
              if (min > max) {
                min = 0;
                max = 0;
              }
              const unit = genericUnitLabel(variable?.quantity, sys);
              const marker =
                currentPeriod == null || values.length === 0
                  ? null
                  : Math.max(0, Math.min(currentPeriod, values.length - 1));
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
                    {unit ? ` (${unit})` : ""}
                  </div>
                  <Sparkline
                    values={values}
                    min={min}
                    max={max}
                    stroke="var(--accent)"
                    times={series?.times}
                    markerIndex={marker}
                    unit={unit}
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
  const sys = useUnitSystem();
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
                value={formatGenericValue(primary.value, primary.quantity, sys)}
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
                      value={formatGenericValue(s.value, s.quantity, sys)}
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
