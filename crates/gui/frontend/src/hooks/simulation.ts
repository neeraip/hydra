/**
 * Simulation execution: run command, live progress events, and simulation
 * parameters ([TIMES] + [OPTIONS], INP-canonical).
 */

import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { invoke, tryInvoke } from "./ipc";
import { useNetworkVersion } from "./NetworkVersionContext";
import type { PumpEnergyRecord } from "./results";

/** Returned by `run_simulation`. Contains only pump energy. */
export interface SimulationResult {
  pumpEnergy: PumpEnergyRecord[];
}

export async function runSimulation(opts?: {
  projectId?: string;
  scenarioId?: string;
  qualityMode?: string;
  traceNode?: string;
}): Promise<SimulationResult | null> {
  return await invoke<SimulationResult | null>("run_simulation", {
    projectId: opts?.projectId ?? null,
    scenarioId: opts?.scenarioId ?? null,
    qualityMode: opts?.qualityMode ?? null,
    traceNode: opts?.traceNode ?? null,
  });
}

// ── Simulation progress events ─────────────────────────────────────────────

export const SIMULATION_PROGRESS_EVENT = "simulation_progress";

export interface SimulationProgressEvent {
  /** The run-queue item UUID; `null` for direct (non-queued) runs. */
  runId: string | null;
  /** "hydraulics" or "quality" */
  phase: string;
  simulatedSeconds: number;
  durationSeconds: number;
  percent: number;
  done: boolean;
  failed: boolean;
  message: string | null;
  /** Whether water-quality is enabled for this simulation run. */
  runQuality: boolean;
}

/** Subscribe to live simulation progress events from the backend.
 *  Returns the unlisten function — call it to unsubscribe. */
export function listenSimulationProgress(
  cb: (e: SimulationProgressEvent) => void,
): Promise<() => void> {
  return listen<SimulationProgressEvent>(SIMULATION_PROGRESS_EVENT, (ev) =>
    cb(ev.payload),
  );
}

// ── Simulation parameters (TIMES + OPTIONS, INP-canonical) ────────────────

/** Mirrors the backend `SimParamsDto`. PDA pressure values are in metres (SI). */
export interface SimParams {
  duration: number;
  hydStep: number;
  qualStep: number;
  patternStep: number;
  reportStep: number;
  startClocktime: number;
  statistic: "series" | "average" | "minimum" | "maximum" | "range";

  headLossFormula: "H-W" | "D-W" | "C-M";
  demandModel: "DDA" | "PDA";
  demandMultiplier: number;
  pdaMinPressure: number;
  pdaRequiredPressure: number;
  pdaPressureExponent: number;

  qualityMode: "none" | "chemical" | "age" | "trace";
  traceNode: string | null;
  chemName: string;
  chemUnits: string;

  maxIter: number;
  flowTol: number;
  headTol: number;
  dampLimit: number;
  checkFreq: number;
  maxCheck: number;
  viscosity: number;
  specificGravity: number;
}

/** Read [TIMES] + [OPTIONS] from `base/model.inp`. `null` if no base INP. */
export async function getSimParams(
  projectId: string,
): Promise<SimParams | null> {
  return (
    (await tryInvoke<SimParams | null>("get_sim_params", { projectId })) ?? null
  );
}

/** Persist new sim params: rewrites base + every scenario INP and marks every
 *  existing result stale. Rejects with the backend error message on failure
 *  so callers can surface it to the user. */
export async function updateSimParams(
  projectId: string,
  params: SimParams,
): Promise<void> {
  await invoke("update_sim_params", { projectId, params });
}

/**
 * React hook that tracks simulation parameters for `projectId`, re-fetching
 * whenever `networkVersion` bumps (i.e. a new network was loaded). Returns
 * `null` until the first fetch resolves or when `projectId` is absent.
 */
export function useSimParams(
  projectId: string | null | undefined,
): SimParams | null {
  const { version: networkVersion } = useNetworkVersion();
  const [params, setParams] = useState<SimParams | null>(null);
  useEffect(() => {
    void networkVersion;
    if (!projectId) {
      setParams(null);
      return;
    }
    let cancelled = false;
    getSimParams(projectId).then((p) => {
      if (!cancelled) setParams(p);
    });
    return () => {
      cancelled = true;
    };
  }, [projectId, networkVersion]);
  return params;
}
