/**
 * Simulation result access: pump energy, analytics, result metadata, and
 * per-period result arrays.
 */

import { tryInvoke } from "./ipc";

// ── Simulation results ────────────────────────────────────────────────

/** Per-pump energy accounting for the full simulation. */
export interface PumpEnergyRecord {
  id: string;
  pctOnline: number;
  avgEfficiency: number;
  avgKwhPerFlow: number;
  avgKw: number;
  peakKw: number;
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
  minPressureNodeId: string;
  minPressureM: number;
  lowPressureCount: number;
  maxVelocityLinkId: string;
  maxVelocityMs: number;
  pressureHistogram: HistogramBucket[];
  velocityHistogram: HistogramBucket[];
  topPipes: TopPipe[];
  tankSeries: TankHeadSeries[];
}

export async function getPumpEnergy(
  projectId: string,
  scenarioId?: string | null,
): Promise<PumpEnergyRecord[]> {
  return (
    (await tryInvoke<PumpEnergyRecord[]>("get_pump_energy", {
      projectId,
      scenarioId: scenarioId ?? null,
    })) ?? []
  );
}

export async function getResultAnalytics(
  projectId: string,
  scenarioId?: string | null,
): Promise<ResultAnalytics | null> {
  return (
    (await tryInvoke<ResultAnalytics | null>("get_result_analytics", {
      projectId,
      scenarioId: scenarioId ?? null,
    })) ?? null
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
  nodeDemand: number[];
  /** Node hydraulic head (m), one entry per node in network order. */
  nodeHead: number[];
  /** Node gauge pressure (m), one entry per node in network order. */
  nodePressure: number[];
  /** Link flow (L/s), one entry per link in network order. */
  linkFlow: number[];
  /** Link mean velocity (m/s), one entry per link in network order. */
  linkVelocity: number[];
  /** Link head loss per unit length (or total for pumps/valves). */
  linkHeadloss: number[];
  /** Link status (0 = closed, 1 = open, etc.) */
  linkStatus: number[];
  /** Per-node quality values. Present only when quality simulation was run. */
  nodeQuality?: number[];
  /** Per-link quality values. Present only when quality simulation was run. */
  linkQuality?: number[];
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
  return (
    (await tryInvoke<ResultMeta | null>("load_result_meta", {
      projectId,
      scenarioId: scenarioId ?? null,
    })) ?? null
  );
}

/**
 * Return flat result arrays for a single reporting period.
 * Values are in SI units (L/s, m, m/s). Returns `null` outside Tauri.
 */
export async function getPeriodResults(
  projectId: string,
  period: number,
  scenarioId?: string | null,
): Promise<PeriodResults | null> {
  return (
    (await tryInvoke<PeriodResults | null>("get_period_results", {
      projectId,
      period,
      scenarioId: scenarioId ?? null,
    })) ?? null
  );
}
