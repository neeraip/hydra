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

export interface PipeRow {
  id: string;
  from: string;
  to: string;
  length: number;
  diameter: number;
  roughness: number;
  velocity: number;
  highVelocity: boolean;
}

export interface PumpRow {
  id: string;
  from: string;
  to: string;
  /** Head-flow curve ID; undefined for constant-power pumps. */
  curve: string | null;
  /** Rated power in kW; defined only for constant-power pumps. */
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

export function useJunctionRows(): JunctionRow[] {
  const nodes = useNodes();
  return useMemo(
    () =>
      nodes
        .filter((n) => n.type === "junction")
        .map((n) => ({
          id: n.id,
          elevation: Math.round((n.elevation ?? 0) * 100) / 100,
          baseDemand: Math.round((n.baseDemand ?? 0) * 100) / 100,
          demand: n.demand ?? 0,
          pressure:
            n.pressure !== null ? Math.round(n.pressure * 10) / 10 : null,
          x: Math.round(n.x * 100) / 100,
          y: Math.round(n.y * 100) / 100,
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
          length: Math.round((l.length ?? 0) * 10) / 10,
          diameter: l.diameter,
          roughness: l.roughness ?? 0,
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
          elevation: Math.round((n.elevation ?? 0) * 100) / 100,
          minLevel: Math.round((n.tankMinLevel ?? 0) * 100) / 100,
          maxLevel: Math.round((n.tankMaxLevel ?? 0) * 100) / 100,
          initialLevel: Math.round((n.tankInitialLevel ?? 0) * 100) / 100,
          diameter:
            n.tankDiameter != null
              ? Math.round(n.tankDiameter * 100) / 100
              : null,
          volumeCurve: n.tankVolumeCurve ?? null,
          x: Math.round(n.x * 100) / 100,
          y: Math.round(n.y * 100) / 100,
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
          head: Math.round((n.elevation ?? 0) * 100) / 100,
          pattern: n.headPattern ?? null,
          x: Math.round(n.x * 100) / 100,
          y: Math.round(n.y * 100) / 100,
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
          diameter: Math.round(l.diameter * 10) / 10,
          setting:
            l.valveSetting != null
              ? Math.round(l.valveSetting * 1000) / 1000
              : null,
          curve: l.valveCurve ?? null,
          velocity: l.velocity,
        })),
    [links],
  );
}

export function useEditorSections() {
  // Intentionally empty — EditorView derives its own sections inline.
  return [] as never[];
}

// ── Cross-section / subcatchment editor data ───────────────────────────────

export interface XSStation {
  station: number;
  elev: number;
  manning?: number;
}
export interface CrossSection {
  id: string;
  reach: string;
  riverStation: number;
  description: string;
  bankLeft: number;
  bankRight: number;
  manningChannel: number;
  manningOverbankL: number;
  manningOverbankR: number;
  ineffective?: { left: number; right: number; elevation: number };
  points: XSStation[];
}
export interface Subcatchment {
  id: string;
  area: number;
  imperv: number;
  width: number;
  slope: number;
  manningImp: number;
  manningPerv: number;
  outletNode: string;
  rainGage: string;
  peakRunoff: number;
}
