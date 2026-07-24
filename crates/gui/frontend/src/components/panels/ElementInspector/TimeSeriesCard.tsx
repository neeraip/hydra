/**
 * Element time-series card for the inspector.
 *
 * When results exist (resultMeta present with more than one reporting
 * period), fetches the selected element's full-simulation series via
 * `get_element_series` and renders one compact interactive chart per key
 * field (nodes: pressure + demand; links: flow + velocity; quality when
 * present). Remaining fields (head / headloss / status) sit behind a small
 * "more fields" toggle.
 *
 * Self-sourcing by design: project / scenario / result identity come from
 * AppContext hooks and the element's network-order index is derived from
 * the NetworkDataContext arrays, because the inspector body receives none
 * of these as props. The currently scrubbed period arrives through the
 * minimal `period-context` (provided by CanvasView) and is drawn as a
 * Sparkline `markerIndex`, clamped to each series' length; outside a
 * timeline context the hook returns `null` and no marker is drawn.
 *
 * Steady-state runs (a single period) render nothing — there is no trend
 * to plot. Fetched series are cached per element id + resultGeneration so
 * re-selecting an element between runs never refetches.
 */

import { useEffect, useMemo, useState } from "react";
import {
  useActiveProject,
  useAppState,
  useSimulation,
} from "../../../AppContext";
import { useCurrentPeriod } from "../../../canvas/period-context";
import {
  type ElementSeries,
  type ElementSeriesField,
  getElementSeries,
  useNetworkData,
} from "../../../hooks";
import { Sparkline } from "../../../pages/project/AnalysisPanel/charts";
import {
  type Quantity,
  toDisplay,
  type UnitSystem,
  unitLabel,
  useUnitSystem,
} from "../../../units";
import { SectionLabel } from "../../ui/SectionLabel";
import {
  elementSeriesCacheKey,
  headlossQuantity,
  LruCache,
} from "./seriesCache";

/** Module-level cache: survives element re-selection and inspector remounts.
 *  `null` entries record a definitive "no series" backend answer. */
const seriesCache = new LruCache<ElementSeries | null>(24);

/** Key fields shown by default, in display order. Quality only exists when a
 *  quality simulation was run — absent fields are simply not rendered. */
const PRIMARY_FIELDS: Record<"node" | "link", string[]> = {
  node: ["pressure", "demand", "quality"],
  link: ["flow", "velocity", "quality"],
};

/** Display quantity per backend field name — all series arrive in SI.
 * Absent fields (status, quality) are unitless. `headloss` is resolved via
 * {@link headlossQuantity} instead: the results file stores it per-1000
 * (m/km) for pipes but as total metres for pumps/valves, so its quantity
 * depends on the link type. */
const FIELD_QUANTITIES: Record<string, Quantity> = {
  pressure: "pressure",
  head: "head",
  demand: "demand",
  flow: "flow",
  velocity: "velocity",
};

function fieldDecimals(name: string): number {
  if (name === "status") return 0;
  if (name === "quality") return 3;
  return 2;
}

function fieldRange(values: number[]): { min: number; max: number } {
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  for (const v of values) {
    if (!Number.isFinite(v)) continue;
    if (v < min) min = v;
    if (v > max) max = v;
  }
  if (min > max) return { min: 0, max: 0 };
  return { min, max };
}

function FieldChart({
  field,
  times,
  sys,
  markerIndex,
  linkType,
}: {
  field: ElementSeriesField;
  times: number[];
  sys: UnitSystem;
  /** Current scrub period, or null when no timeline context exists. */
  markerIndex: number | null;
  /** "pipe" | "pump" | "valve" for link series; undefined for node series. */
  linkType?: string;
}) {
  const name = field.name.toLowerCase();
  const quantity =
    name === "headloss" ? headlossQuantity(linkType) : FIELD_QUANTITIES[name];
  // Convert at the render boundary only — the cached series stays SI.
  const values = useMemo(
    () =>
      quantity && sys === "us"
        ? field.values.map((v) => toDisplay(v, quantity, sys))
        : field.values,
    [field.values, quantity, sys],
  );
  const { min, max } = fieldRange(values);
  const unit = quantity ? unitLabel(quantity, sys) : "";
  // Clamp the scrub marker to this series' length (a shorter series holds
  // its last point rather than dropping the marker).
  const marker =
    markerIndex == null || values.length === 0
      ? null
      : Math.max(0, Math.min(markerIndex, values.length - 1));
  return (
    <div>
      <div
        style={{
          fontSize: 10,
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: "0.06em",
          marginBottom: 3,
        }}
      >
        {field.name}
        {unit ? ` (${unit})` : ""}
      </div>
      <Sparkline
        values={values}
        min={min}
        max={max}
        stroke="var(--accent)"
        times={times}
        markerIndex={marker}
        unit={unit}
        decimals={fieldDecimals(field.name)}
        height={40}
      />
    </div>
  );
}

export function TimeSeriesCard({
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
  const sys = useUnitSystem();
  // Scrub position from CanvasView's provider; null outside a timeline.
  const currentPeriod = useCurrentPeriod();

  const [series, setSeries] = useState<ElementSeries | null>(null);
  const [loading, setLoading] = useState(false);
  const [showAll, setShowAll] = useState(false);

  const projectId = project?.id ?? null;
  const periods = resultMeta?.times.length ?? 0;

  // Translate the selected element id to its network-order index — the
  // backend addresses series by index, not id.
  const index = useMemo(() => {
    const arr: Array<{ id: string }> = kind === "node" ? nodes : links;
    return arr.findIndex((el) => el.id === elementId);
  }, [kind, nodes, links, elementId]);
  // Link type disambiguates the headloss quantity (m/km for pipes, total m
  // for pumps/valves) — see `headlossQuantity`.
  const linkType =
    kind === "link" && index >= 0 ? links[index]?.type : undefined;

  // Steady-state (≤ 1 period), no project, or unknown element: no card.
  const enabled = projectId != null && periods > 1 && index >= 0;

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
    elementId,
    index,
  ]);

  if (!enabled) return null;

  const primaryNames = PRIMARY_FIELDS[kind];
  const fields = series?.fields ?? [];
  const primaryFields = primaryNames
    .map((name) => fields.find((f) => f.name.toLowerCase() === name))
    .filter((f): f is ElementSeriesField => f != null);
  const extraFields = fields.filter(
    (f) => !primaryNames.includes(f.name.toLowerCase()),
  );
  const shown = showAll ? [...primaryFields, ...extraFields] : primaryFields;

  // Command missing / errored / degenerate payload: skip the card entirely.
  if (
    !loading &&
    (series == null || series.times.length < 2 || shown.length === 0)
  ) {
    return null;
  }

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
              fontSize: 12,
              color: "var(--text-secondary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            Loading time series…
          </span>
        ) : (
          <>
            {shown.map((f) => (
              <FieldChart
                key={f.name}
                field={f}
                times={series?.times ?? []}
                sys={sys}
                markerIndex={currentPeriod}
                linkType={linkType}
              />
            ))}
            {extraFields.length > 0 && (
              <button
                type="button"
                onClick={() => setShowAll((v) => !v)}
                style={{
                  alignSelf: "flex-start",
                  background: "transparent",
                  border: "none",
                  padding: 0,
                  cursor: "pointer",
                  fontSize: 11,
                  color: "var(--text-secondary)",
                  fontFamily: "var(--font-ui)",
                  textDecoration: "underline",
                  textUnderlineOffset: 2,
                }}
              >
                {showAll
                  ? "Show fewer fields"
                  : `Show ${extraFields.length} more field${
                      extraFields.length === 1 ? "" : "s"
                    }`}
              </button>
            )}
          </>
        )}
      </div>
    </>
  );
}
