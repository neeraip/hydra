export type NodeType = "junction" | "tank" | "reservoir";
export type LinkType = "pipe" | "pump" | "valve";

export interface Node {
  id: string;
  type: NodeType;
  x: number;
  y: number;
  /** Elevation in metres; 0 when not yet loaded from backend. */
  elevation?: number;
  /** Sum of base demands in L/s; 0 when not yet loaded from backend. */
  baseDemand?: number;
  pressure: number | null;
  demand: number | null;
  /** Hydraulic head in metres. `null` when no simulation has run. */
  head?: number | null;
  /** Water quality value (units depend on quality mode). `null` when no quality simulation was run. */
  quality?: number | null;
  // Tank-only
  tankMinLevel?: number | null;
  tankMaxLevel?: number | null;
  tankInitialLevel?: number | null;
  tankDiameter?: number | null;
  tankVolumeCurve?: string | null;
  // Reservoir-only
  headPattern?: string | null;
}

export interface Link {
  id: string;
  type: LinkType;
  fromId: string;
  toId: string;
  velocity: number;
  /** Flow in L/s for the current reporting period. `null` when no simulation has run. */
  flow?: number | null;
  /**
   * Link status from the simulation result (Hydra OUT-file codes):
   * 3 = Open, 2 = Closed, 4 = Active, 0 = XHead, 1 = TempClosed, 6 = XFcv, 7 = XPressure.
   * `null` when no simulation has run.
   */
  status?: number | null;
  diameter: number;
  /** Water quality value along the link. `null` when no quality simulation was run. */
  quality?: number | null;
  /** Pipe length in metres; 0 for pumps/valves. */
  length?: number;
  /** Hazen-Williams roughness coefficient; 0 for pumps/valves. */
  roughness?: number;
  // Pump-only
  pumpCurve?: string | null;
  pumpPowerKw?: number | null;
  pumpSpeed?: number | null;
  // Valve-only
  /** "PRV" | "PSV" | "FCV" | "TCV" | "GPV" | "PBV" | "PCV"; null for non-valves. */
  valveType?: string | null;
  /** Setting in display units: m for PRV/PSV/PBV, L/s for FCV, dimensionless for TCV. */
  valveSetting?: number | null;
  /** Curve ID for GPV/PCV; null otherwise. */
  valveCurve?: string | null;
}

export interface Pattern {
  id: string;
  /** Dimensionless multipliers [F₀, F₁, …, F_{L−1}]. */
  multipliers: number[];
}
