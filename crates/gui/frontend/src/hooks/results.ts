/**
 * Simulation result access: pump energy, analytics, result metadata, and
 * per-period result arrays.
 */

import { formatDecimal, significantDecimals } from "../numberFormat";
import { tryInvoke, tryInvokeOr } from "./ipc";

// ── Simulation results ────────────────────────────────────────────────

/** Per-pump energy accounting for the full simulation. */
export interface PumpEnergyRecord {
  id: string;
  pctOnline: number;
  avgEfficiency: number;
  avgKwhPerFlow: number;
  avgKw: number;
  peakKw: number;
  /** Total energy consumed over the simulation (kWh). */
  totalKwh: number;
  /** Total energy cost over the simulation; `null` when no price data. */
  totalCost: number | null;
}

// ── Element time series ─────────────────────────────────────────────────────

/** One named per-period series for an element (SI display units). */
export interface ElementSeriesField {
  name: string;
  values: number[];
}

/**
 * Full-simulation time series for a single node or link.
 * Node field order: pressure, head, demand[, quality].
 * Link field order: flow, velocity, headloss, status[, quality].
 */
export interface ElementSeries {
  /** Snapshot times in seconds from the start of the simulation. */
  times: number[];
  fields: ElementSeriesField[];
}

/**
 * Which class of element a series is addressed against.
 *
 * One name for the three classes, so widening the set is a single edit
 * rather than four literal unions that can drift apart. `region` was added
 * once the drainage reader's subcatchment records were routed through —
 * engines with no areal elements simply never ask for it.
 */
export type ElementSeriesKind = "node" | "link" | "region";

/**
 * Fetch the per-period series for one element by its network-order index.
 * Returns `null` outside Tauri, when the command is missing/fails, or when
 * no results exist for the project/scenario.
 */
export async function getElementSeries(
  projectId: string,
  scenarioId: string | null | undefined,
  kind: ElementSeriesKind,
  index: number,
): Promise<ElementSeries | null> {
  return tryInvoke<ElementSeries | null>("get_element_series", {
    projectId,
    scenarioId: scenarioId ?? null,
    kind,
    index,
  });
}

/** Per-pump energy usage for the target's completed run. */
export async function getPumpEnergy(
  projectId: string,
  scenarioId?: string | null,
): Promise<PumpEnergyRecord[]> {
  return tryInvokeOr<PumpEnergyRecord[]>(
    "get_pump_energy",
    { projectId, scenarioId: scenarioId ?? null },
    [],
  );
}

// ── Result metadata + period results ──────────────────────────────────────────
//
// These map to the `load_result_meta` / `get_period_results` Tauri commands.
// `loadResultMeta` reads only the tiny 72-byte header + epilog from `results.out`
// and returns snapshot times with global min/max ranges — fast on any size file.
// `getPeriodResults` seeks directly to one period and returns flat SI arrays.

export interface ResultRanges {
  pressureMin: number;
  pressureMax: number;
  headMin: number;
  headMax: number;
  demandMin: number;
  demandMax: number;
  flowMin: number;
  flowMax: number;
  velocityMin: number;
  velocityMax: number;
  /** Present only when quality simulation was run. */
  qualityMin?: number;
  qualityMax?: number;
}

/**
 * How a target's per-period values are delivered, and therefore which
 * decoder to call and which colour path the canvas takes.
 *
 * A named decision rather than an inline `if`, because it is exactly the
 * kind that fails silently: choosing wrong yields no error, just a canvas
 * with nothing on it. Two consumers ask this question — the fetch effect
 * and the canvas — and when they derived it separately from `generic`
 * being present, both were wrong together.
 *
 * `"generic"` — the variable-major catalog payload.
 * `"fixed"` — the engine's own fixed period arrays.
 * `"none"` — no per-period data for this target at all.
 */
export type ResultsPath = "generic" | "fixed" | "none";

export function resultsPath(
  meta: Pick<ResultMeta, "hasPeriodData" | "genericPeriods"> | null | undefined,
): ResultsPath {
  if (!meta || meta.hasPeriodData === false) return "none";
  // The catalog is deliberately not consulted: every engine publishes one,
  // and it says what the results *contain*, never how they are encoded.
  return meta.genericPeriods ? "generic" : "fixed";
}

/** §5 quantity descriptor accompanying a variable's SI values — everything
 * needed to convert to the active display system at the render boundary. */
export interface GenericQuantity {
  key: string;
  siLabel: string;
  usLabel: string;
  siToUsScale: number;
  siToUsOffset: number;
  siDecimals: number;
  usDecimals: number;
}

/**
 * One engine-described result variable with its per-run value range —
 * everything a legend needs, authored by the engine's catalog. Values and
 * ranges are SI; `quantity` carries the display conversion.
 */
/**
 * How remarkable one categorical state is (contract §6.1).
 *
 * A claim about the domain, not about presentation — only the engine knows
 * that a closed pipe is an abnormal condition. Absent where the states are
 * merely a partition (a material, a land-use class) with no abnormal
 * member; absent is a real answer, not a missing one.
 */
export type CategorySeverity = "nominal" | "caution" | "alarm";

/** One discrete state of a categorical variable, as the engine named it. */
export interface RampCategory {
  /** The value the result series stores for this state. */
  value: number;
  label: string;
  severity?: CategorySeverity;
}

/**
 * How a variable's values map to a colour scale (contract §6.1) — a shape
 * statement from the engine, never a colour. Carried in the contract's own
 * tagged form so `categorical` keeps its engine-authored states: a status
 * variable stripped of its labels can only be drawn as a meaningless
 * gradient over status codes.
 */
export type RampHint =
  | { type: "sequential" }
  | { type: "diverging" }
  /** Classed against a criterion's thresholds; `criterion` is the key
   *  (spec §7.1) whose valuation supplies them. Named by the engine
   *  because matching criteria to variables by quantity is a guess —
   *  which once offered a drainage map water-distribution numbers. */
  | { type: "banded"; criterion: string }
  | { type: "categorical"; items: RampCategory[] };

export interface GenericVariable {
  id: string;
  label: string;
  /** Engine-authored compact notation (Q, y, Ø) for space-starved
   * surfaces; absent = the application derives a fallback. */
  symbol?: string;
  quantity?: GenericQuantity;
  ramp: RampHint;
  min: number;
  max: number;
}

/**
 * Which variable of a class is on show: the one chosen, or the first
 * the catalog publishes.
 *
 * One rule, in one place, because two places once disagreed about it.
 * The legend resolved the surface class over its own merged list while
 * the canvas resolved it over the run's alone, so the legend named the
 * ground while the map painted depth — the label and the picture
 * describing different things, which is worse than either being absent.
 */
export function selectedVariable(
  variables: GenericVariable[],
  id?: string,
): GenericVariable | undefined {
  return variables.find((v) => v.id === id) ?? variables[0];
}

/** SI value → the active display system. */
export function genericToDisplay(
  value: number,
  quantity: GenericQuantity | undefined,
  sys: "si" | "us",
): number {
  if (!quantity || sys === "si") return value;
  return value * quantity.siToUsScale + quantity.siToUsOffset;
}

/** Unit label for the active display system, or undefined when unitless. */
export function genericUnitLabel(
  quantity: GenericQuantity | undefined,
  sys: "si" | "us",
): string | undefined {
  if (!quantity) return undefined;
  return sys === "us" ? quantity.usLabel : quantity.siLabel;
}

/**
 * Display string for one SI value: converted to the active system and shown
 * to significant-figure precision, with the quantity's declared decimals as
 * a floor. The unit label is appended unless `withUnit` is false.
 */
export function formatGenericValue(
  value: number | null | undefined,
  quantity: GenericQuantity | undefined,
  sys: "si" | "us",
  withUnit = true,
): string {
  if (value == null || !Number.isFinite(value)) return "—";
  const v = genericToDisplay(value, quantity, sys);
  // The quantity's declared decimals are a floor, not the answer: they say
  // how coarsely the engine is willing to be read, and a value smaller than
  // that resolution still has to be legible. Two decimals rendered a
  // 0.000471 lateral inflow as "0.00".
  const floor = quantity
    ? sys === "us"
      ? quantity.usDecimals
      : quantity.siDecimals
    : 0;
  const text = formatDecimal(v, significantDecimals(v, floor));
  const unit = withUnit ? genericUnitLabel(quantity, sys) : undefined;
  return unit ? `${text} ${unit}` : text;
}

/** The engine-described result catalog for one run, per element class. */
export interface GenericResultMeta {
  pointVars: GenericVariable[];
  polylineVars: GenericVariable[];
  regionVars: GenericVariable[];
}

export interface ResultMeta {
  /** Snapshot times in seconds from the start of the simulation. */
  times: number[];
  /** Whether per-period arrays exist for this target; false = the timeline
   * steps but the canvas stays uncoloured (engine provider pending). */
  hasPeriodData?: boolean;
  ranges: ResultRanges;
  /** Quality mode used: `"none"` | `"chemical"` | `"age"` | `"trace"`. */
  qualityMode: string;
  /**
   * Topology digest (16 lowercase hex chars) of the network the results were
   * produced from. Absent for pre-digest `.out` files — the topology match is
   * then unknown and no staleness gating applies.
   */
  networkDigest?: string | null;
  /**
   * Wall-clock start of the run that produced these results, milliseconds
   * since the Unix epoch — from the app's `run.json` beside the results.
   * Absent for results written before the stamp existed.
   */
  startedAtMs?: number | null;
  /** Wall-clock finish of the same run, on the same terms. */
  finishedAtMs?: number | null;
  /**
   * Present for engines whose results flow through the generic
   * variable-keyed payload (uds). When set, `get_period_results` returns
   * the generic layout and `decodeGenericPeriodValues` is the decoder;
   * the fixed `ranges` above are then all-zero and unused.
   */
  generic?: GenericResultMeta;
  /**
   * Whether per-period values arrive in the generic variable-major payload
   * rather than the fixed wds arrays.
   *
   * Separate from `generic` on purpose. Every engine publishes a catalog —
   * that is what the legend renders — but the catalog says nothing about
   * how a period is encoded on the wire. Reading `generic` as "the values
   * come from the catalog path" routed wds onto a payload nothing serves
   * for it, and its canvas fell back to the network-at-rest palette with
   * no error raised anywhere.
   */
  genericPeriods?: boolean;
}

/** Result of comparing the results' stored topology digest against the live
 * model's digest — see {@link compareTopologyDigests}. */
export type TopologyDigestMatch = "match" | "stale" | "unknown";

/**
 * Pure staleness decision for topology-addressed results.
 *
 * `"stale"` only when BOTH digests are known and differ; any missing side
 * (pre-digest `.out` file, digest fetch unavailable) yields `"unknown"`,
 * which callers must treat exactly like today's ungated behaviour — old
 * results are never punished for lacking a digest.
 */
export function compareTopologyDigests(
  metaDigest: string | null | undefined,
  liveDigest: string | null | undefined,
): TopologyDigestMatch {
  if (!metaDigest || !liveDigest) return "unknown";
  return metaDigest === liveDigest ? "match" : "stale";
}

export interface PeriodResults {
  /** Node demand (L/s), one entry per node in network order. */
  nodeDemand: Float32Array;
  /** Node hydraulic head (m), one entry per node in network order. */
  nodeHead: Float32Array;
  /** Node gauge pressure (m), one entry per node in network order. */
  nodePressure: Float32Array;
  /** Link flow (L/s), one entry per link in network order. */
  linkFlow: Float32Array;
  /** Link mean velocity (m/s), one entry per link in network order. */
  linkVelocity: Float32Array;
  /** Link head loss per unit length (or total for pumps/valves). */
  linkHeadloss: Float32Array;
  /** Link status (0 = closed, 1 = open, etc.) */
  linkStatus: Float32Array;
  /** Per-node quality values. Present only when quality simulation was run. */
  nodeQuality?: Float32Array;
  /** Per-link quality values. Present only when quality simulation was run. */
  linkQuality?: Float32Array;
}

/** Set in the binary header's flags word when quality arrays are appended. */
const PERIOD_RESULTS_FLAG_QUALITY = 1;

function periodResultsError(detail: string): Error {
  return new Error(`period results decode failed: ${detail}`);
}

/**
 * Decode the compact little-endian binary layout produced by the backend's
 * `encode_period_results`:
 *
 * ```text
 * u32 nNodes | u32 nLinks | u32 flags |
 * f32×nNodes nodeDemand | nodeHead | nodePressure |
 * f32×nLinks linkFlow | linkVelocity | linkHeadloss | linkStatus |
 * [f32×nNodes nodeQuality | f32×nLinks linkQuality]   (flags bit 0)
 * ```
 *
 * The typed arrays are zero-copy views over the response buffer.
 *
 * Returns `null` only for a zero-byte buffer — the "no data" representation.
 * (In practice absent results never reach this decoder: `get_period_results`
 * errors when `results.out` is missing, and callers pre-check result
 * metadata.) Any non-empty malformed or truncated buffer throws a
 * descriptive error so a frontend/backend layout mismatch surfaces loudly
 * instead of masquerading as "no results".
 *
 * Exported for tests — production callers go through `getPeriodResults`.
 */
export function decodePeriodResults(buf: ArrayBuffer): PeriodResults | null {
  const HEADER_BYTES = 12;
  if (buf.byteLength === 0) return null;
  if (buf.byteLength < HEADER_BYTES) {
    throw periodResultsError(`buffer too short (${buf.byteLength} bytes)`);
  }
  const view = new DataView(buf);
  const nNodes = view.getUint32(0, true);
  const nLinks = view.getUint32(4, true);
  const flags = view.getUint32(8, true);
  const hasQuality = (flags & PERIOD_RESULTS_FLAG_QUALITY) !== 0;

  const expected =
    HEADER_BYTES +
    4 * (3 * nNodes + 4 * nLinks) +
    (hasQuality ? 4 * (nNodes + nLinks) : 0);
  if (buf.byteLength < expected) {
    throw periodResultsError(
      `truncated buffer (${buf.byteLength} bytes for ${nNodes} nodes + ${nLinks} links${
        hasQuality ? " + quality" : ""
      }, expected ${expected})`,
    );
  }

  let offset = HEADER_BYTES;
  const take = (len: number): Float32Array => {
    const arr = new Float32Array(buf, offset, len);
    offset += 4 * len;
    return arr;
  };

  const result: PeriodResults = {
    nodeDemand: take(nNodes),
    nodeHead: take(nNodes),
    nodePressure: take(nNodes),
    linkFlow: take(nLinks),
    linkVelocity: take(nLinks),
    linkHeadloss: take(nLinks),
    linkStatus: take(nLinks),
  };
  if (hasQuality) {
    result.nodeQuality = take(nNodes);
    result.linkQuality = take(nLinks);
  }
  return result;
}

/**
 * One period's values for every catalog variable, decoded from the generic
 * payload: `points[v][i]` is variable `v` (in `GenericResultMeta` order)
 * for snapshot point `i` (the canvas's element order). `NaN` marks an
 * element the results file does not report.
 */
export interface GenericPeriodValues {
  points: Float32Array[];
  polylines: Float32Array[];
  regions: Float32Array[];
}

/**
 * Decode the generic period payload (backend `encode_generic_period`):
 *
 * ```text
 * u32 nPoints | u32 nPolylines | u32 nRegions |
 * u32 nPointVars | u32 nPolylineVars | u32 nRegionVars |
 * f32 arrays, variable-major, catalog order: points, polylines, regions
 * ```
 *
 * Returns `null` for a zero-byte buffer (no results); throws on a
 * malformed buffer so a layout mismatch surfaces loudly.
 * Exported for tests — production callers go through
 * `getGenericPeriodValues`.
 */
export function decodeGenericPeriodValues(
  buf: ArrayBuffer,
): GenericPeriodValues | null {
  const HEADER_BYTES = 24;
  if (buf.byteLength === 0) return null;
  if (buf.byteLength < HEADER_BYTES) {
    throw periodResultsError(`buffer too short (${buf.byteLength} bytes)`);
  }
  const view = new DataView(buf);
  const counts = [0, 1, 2, 3, 4, 5].map((i) => view.getUint32(4 * i, true));
  const [
    nPoints,
    nPolylines,
    nRegions,
    nPointVars,
    nPolylineVars,
    nRegionVars,
  ] = counts;
  const expected =
    HEADER_BYTES +
    4 *
      (nPointVars * nPoints +
        nPolylineVars * nPolylines +
        nRegionVars * nRegions);
  if (buf.byteLength < expected) {
    throw periodResultsError(
      `truncated generic buffer (${buf.byteLength} bytes, expected ${expected})`,
    );
  }
  let offset = HEADER_BYTES;
  const takeClass = (nVars: number, nElements: number): Float32Array[] => {
    const arrays: Float32Array[] = [];
    for (let v = 0; v < nVars; v += 1) {
      arrays.push(new Float32Array(buf, offset, nElements));
      offset += 4 * nElements;
    }
    return arrays;
  };
  return {
    points: takeClass(nPointVars, nPoints),
    polylines: takeClass(nPolylineVars, nPolylines),
    regions: takeClass(nRegionVars, nRegions),
  };
}

/**
 * Fetch one period of the generic variable-keyed payload. Same backend
 * command as `getPeriodResults` — the engine decides the layout, and the
 * caller picks this decoder when `ResultMeta.generic` is present.
 */
export async function getGenericPeriodValues(
  projectId: string,
  period: number,
  scenarioId?: string | null,
): Promise<GenericPeriodValues | null> {
  const buf = await tryInvoke<ArrayBuffer>("get_period_results", {
    projectId,
    period,
    scenarioId: scenarioId ?? null,
  });
  if (buf === null) return null;
  if (!(buf instanceof ArrayBuffer)) {
    const err = periodResultsError(
      `get_period_results returned unexpected payload type ${typeof buf} (expected ArrayBuffer)`,
    );
    console.error("[results]", err);
    throw err;
  }
  if (buf.byteLength === 0) return null;
  try {
    return decodeGenericPeriodValues(buf);
  } catch (err) {
    console.error("[results] generic period decode failed:", err);
    throw err;
  }
}

/**
 * Return snapshot times and global result ranges for a project or scenario.
 * Reads only the header + epilog of `results.out` — never the full file.
 * Returns `null` when running outside Tauri or when no results exist yet.
 */
export async function loadResultMeta(
  projectId: string,
  scenarioId?: string | null,
): Promise<ResultMeta | null> {
  return tryInvokeOr<ResultMeta | null>(
    "load_result_meta",
    { projectId, scenarioId: scenarioId ?? null },
    null,
  );
}

/**
 * Return the topology digest (16 lowercase hex chars) of the CURRENT model
 * for a project/scenario — including unsaved in-memory edits when the backend
 * cache holds that target. Compared against `ResultMeta.networkDigest` to
 * detect results that predate the live topology. Returns `null` outside
 * Tauri or when the command is unavailable/fails (treated as "unknown").
 */
export async function getNetworkDigest(
  projectId: string,
  scenarioId?: string | null,
): Promise<string | null> {
  return tryInvokeOr<string | null>(
    "get_network_digest",
    { projectId, scenarioId: scenarioId ?? null },
    null,
  );
}

/**
 * Return flat result arrays for a single reporting period.
 *
 * The backend responds with a compact binary payload (~1.3 MB at 46k nodes +
 * 46k links vs ~3.2 MB as JSON) that is decoded here into zero-copy
 * `Float32Array` views. Values are in SI units (L/s, m, m/s).
 *
 * Returns `null` outside Tauri or when the command itself fails (reported
 * via `onIpcError`). Throws when the payload cannot be decoded or has an
 * unexpected type (frontend/backend contract break); the error is also
 * logged here so a caller that drops the rejection still leaves a
 * diagnosable trail.
 */
export async function getPeriodResults(
  projectId: string,
  period: number,
  scenarioId?: string | null,
): Promise<PeriodResults | null> {
  const buf = await tryInvoke<ArrayBuffer>("get_period_results", {
    projectId,
    period,
    scenarioId: scenarioId ?? null,
  });
  // `null` = outside Tauri or the command failed (reported via onIpcError).
  if (buf === null) return null;
  if (!(buf instanceof ArrayBuffer)) {
    const err = periodResultsError(
      `get_period_results returned unexpected payload type ${typeof buf} (expected ArrayBuffer)`,
    );
    console.error("[results]", err);
    throw err;
  }
  // Empty payload = `results.out` does not exist yet (target not simulated).
  // The backend returns this instead of erroring, so treat it as "no results".
  if (buf.byteLength === 0) return null;
  try {
    return decodePeriodResults(buf);
  } catch (err) {
    console.error("[results] get_period_results decode failed:", err);
    throw err;
  }
}
