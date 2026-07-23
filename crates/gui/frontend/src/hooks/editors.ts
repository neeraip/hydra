/**
 * Editor row types + mappers derived from the loaded network, plus
 * cross-section / subcatchment editor data types.
 */

import { useMemo } from "react";
import { PRESSURE_THRESHOLD } from "../types";
import { useLinks, useNodes } from "./network";

// ── Editor rows ────────────────────────────────────────────────────────────

export interface JunctionRow {
  id: string;
  /** Elevation in metres. */
  elevation: number;
  /** Base demand in L/s. */
  baseDemand: number;
  demand: number;
  /** Gauge pressure in metres, or null when no simulation results are available. */
  pressure: number | null;
  x: number;
  y: number;
  /** True only when a simulation result exists and pressure is below threshold. */
  belowThreshold: boolean;
}

/** Pipe initial [STATUS] as carried on the network snapshot. */
export type PipeInitialStatus = "open" | "closed" | "cv";

/**
 * Default a link's optional initial status for display: absent (old
 * snapshots, or non-pipe links which never carry it) means "open".
 */
export function defaultPipeInitialStatus(
  s: PipeInitialStatus | undefined,
): PipeInitialStatus {
  return s ?? "open";
}

export interface PipeRow {
  id: string;
  from: string;
  to: string;
  length: number;
  diameter: number;
  roughness: number;
  /** Initial [STATUS]; "open" when the snapshot doesn't carry one. */
  initialStatus: PipeInitialStatus;
  velocity: number;
  highVelocity: boolean;
}

export interface PumpRow {
  id: string;
  from: string;
  to: string;
  /** Head-flow curve ID; null for constant-power pumps. */
  curve: string | null;
  /** Rated power in kW; non-null only for constant-power pumps. */
  powerKw: number | null;
  /** Initial relative speed (1.0 = rated). */
  speed: number;
  /** Post-simulation velocity in m/s; 0 before first run. */
  velocity: number;
}

export interface TankRow {
  id: string;
  elevation: number;
  minLevel: number;
  maxLevel: number;
  initialLevel: number;
  /** Tank diameter in m; null for volume-curve tanks. */
  diameter: number | null;
  /** Volume curve ID; null for cylindrical tanks. */
  volumeCurve: string | null;
  x: number;
  y: number;
}

export interface ReservoirRow {
  id: string;
  /** Fixed hydraulic head in m (= elevation for a simple reservoir). */
  head: number;
  /** Head pattern ID; null when unset. */
  pattern: string | null;
  x: number;
  y: number;
}

export interface ValveRow {
  id: string;
  from: string;
  to: string;
  /** "PRV" | "PSV" | "FCV" | "TCV" | "GPV" | "PBV" | "PCV" */
  valveType: string;
  /** Nominal diameter in mm. */
  diameter: number;
  /**
   * Setting in display units: m for PRV/PSV/PBV, L/s for FCV, dimensionless
   * for TCV.  `null` for GPV/PCV (curve-based).
   */
  setting: number | null;
  /** Curve ID for GPV/PCV valve types; null otherwise. */
  curve: string | null;
  /** Post-simulation velocity in m/s; 0 before first run. */
  velocity: number;
}

/** Display rounding for editor rows: 1 / 2 / 3 decimal places. */
const round1 = (v: number): number => Math.round(v * 10) / 10;
const round2 = (v: number): number => Math.round(v * 100) / 100;
const round3 = (v: number): number => Math.round(v * 1000) / 1000;

export function useJunctionRows(): JunctionRow[] {
  const nodes = useNodes();
  return useMemo(
    () =>
      nodes
        .filter((n) => n.type === "junction")
        .map((n) => ({
          id: n.id,
          elevation: round2(n.elevation ?? 0),
          baseDemand: round2(n.baseDemand ?? 0),
          demand: n.demand ?? 0,
          pressure: n.pressure !== null ? round1(n.pressure) : null,
          x: round2(n.x),
          y: round2(n.y),
          belowThreshold:
            n.pressure !== null && n.pressure < PRESSURE_THRESHOLD,
        })),
    [nodes],
  );
}

export function usePipeRows(): PipeRow[] {
  const links = useLinks();
  return useMemo(
    () =>
      links
        .filter((l) => l.type === "pipe")
        .map((l) => ({
          id: l.id,
          from: l.fromId,
          to: l.toId,
          length: round1(l.length ?? 0),
          diameter: l.diameter,
          roughness: l.roughness ?? 0,
          initialStatus: defaultPipeInitialStatus(l.initialStatus),
          velocity: l.velocity,
          highVelocity: l.velocity > 1.0,
        })),
    [links],
  );
}

export function usePumpRows(): PumpRow[] {
  const links = useLinks();
  return useMemo(
    () =>
      links
        .filter((l) => l.type === "pump")
        .map((l) => ({
          id: l.id,
          from: l.fromId,
          to: l.toId,
          curve: l.pumpCurve ?? null,
          powerKw: l.pumpPowerKw ?? null,
          speed: l.pumpSpeed ?? 1.0,
          velocity: l.velocity,
        })),
    [links],
  );
}

export function useTankRows(): TankRow[] {
  const nodes = useNodes();
  return useMemo(
    () =>
      nodes
        .filter((n) => n.type === "tank")
        .map((n) => ({
          id: n.id,
          elevation: round2(n.elevation ?? 0),
          minLevel: round2(n.tankMinLevel ?? 0),
          maxLevel: round2(n.tankMaxLevel ?? 0),
          initialLevel: round2(n.tankInitialLevel ?? 0),
          diameter: n.tankDiameter != null ? round2(n.tankDiameter) : null,
          volumeCurve: n.tankVolumeCurve ?? null,
          x: round2(n.x),
          y: round2(n.y),
        })),
    [nodes],
  );
}

export function useReservoirRows(): ReservoirRow[] {
  const nodes = useNodes();
  return useMemo(
    () =>
      nodes
        .filter((n) => n.type === "reservoir")
        .map((n) => ({
          id: n.id,
          head: round2(n.elevation ?? 0),
          pattern: n.headPattern ?? null,
          x: round2(n.x),
          y: round2(n.y),
        })),
    [nodes],
  );
}

export function useValveRows(): ValveRow[] {
  const links = useLinks();
  return useMemo(
    () =>
      links
        .filter((l) => l.type === "valve")
        .map((l) => ({
          id: l.id,
          from: l.fromId,
          to: l.toId,
          valveType: l.valveType ?? "PRV",
          diameter: round1(l.diameter),
          setting: l.valveSetting != null ? round3(l.valveSetting) : null,
          curve: l.valveCurve ?? null,
          velocity: l.velocity,
        })),
    [links],
  );
}

// ── Cross-section / subcatchment editor data ───────────────────────────────

export interface XSStation {
  station: number;
  elev: number;
  manning?: number;
}
