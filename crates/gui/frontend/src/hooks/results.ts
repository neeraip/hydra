/**
 * Simulation result access: pump energy, analytics, result metadata, and
 * per-period result arrays.
 */

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
 * Fetch the per-period series for one element by its network-order index.
 * Returns `null` outside Tauri, when the command is missing/fails, or when
 * no results exist for the project/scenario.
 */
export async function getElementSeries(
  projectId: string,
  scenarioId: string | null | undefined,
  kind: "node" | "link",
  index: number,
): Promise<ElementSeries | null> {
  return tryInvoke<ElementSeries | null>("get_element_series", {
    projectId,
    scenarioId: scenarioId ?? null,
    kind,
    index,
  });
}

// ── Analytics DTOs ──────────────────────────────────────────────────────────

export interface MassBalance {
  inflowM3: number;
  outflowM3: number;
  balancePct: number;
  series: number[];
}

export interface HistogramBucket {
  lo: number;
  hi: number;
  count: number;
}

export interface TopPipe {
  id: string;
  fromId: string;
  toId: string;
  diameterMm: number;
  maxVelocityMs: number;
}

export interface TankHeadSeries {
  nodeId: string;
  head: number[];
}

export interface ResultAnalytics {
  periodCount: number;
  nodeCount: number;
  linkCount: number;
  massBalance: MassBalance;
  /** Absent (omitted or null) when no valid pressure data exists. */
  minPressureNodeId?: string | null;
  /** Absent (omitted or null) when no valid pressure data exists. */
  minPressureM?: number | null;
  lowPressureCount: number;
  /** Absent (omitted or null) when no valid velocity data exists. */
  maxVelocityLinkId?: string | null;
  /** Absent (omitted or null) when no valid velocity data exists. */
  maxVelocityMs?: number | null;
  pressureHistogram: HistogramBucket[];
  velocityHistogram: HistogramBucket[];
  topPipes: TopPipe[];
  tankSeries: TankHeadSeries[];
}

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

export async function getResultAnalytics(
  projectId: string,
  scenarioId?: string | null,
): Promise<ResultAnalytics | null> {
  return tryInvokeOr<ResultAnalytics | null>(
    "get_result_analytics",
    { projectId, scenarioId: scenarioId ?? null },
    null,
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

export interface ResultMeta {
  /** Snapshot times in seconds from the start of the simulation. */
  times: number[];
  ranges: ResultRanges;
  /** Quality mode used: `"none"` | `"chemical"` | `"age"` | `"trace"`. */
  qualityMode: string;
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
  try {
    return decodePeriodResults(buf);
  } catch (err) {
    console.error("[results] get_period_results decode failed:", err);
    throw err;
  }
}
