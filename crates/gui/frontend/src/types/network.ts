/** The wds kinds are enumerated for autocomplete; other engines contribute
 * their own kind ids (snapshot v4), resolved against the engine catalog. */
export type NodeType = "junction" | "tank" | "reservoir" | (string & {});
export type LinkType = "pipe" | "pump" | "valve" | (string & {});

/** An areal element (snapshot v4): a polygon boundary with an optional
 * discharge reference — a subcatchment, in urban-drainage terms. */
export interface Region {
  id: string;
  type: string;
  /** Engine-generic current-period values keyed by catalog variable id,
   * merged by CanvasView for the rail list. `null` = not reported. */
  resultValues?: Record<string, number | null>;
  /** Boundary ring [x, y] in source-CRS coordinates. */
  ring: Array<[number, number]>;
  /** Id of the point element this region discharges to, if any. */
  outletId: string | null;
}

export interface Node {
  id: string;
  type: NodeType;
  x: number;
  y: number;
  /** Elevation in metres; absent when the engine's snapshot carries no
   * attribute data (v4) — never fabricate a 0 for it. */
  elevation?: number;
  /** Sum of base demands in L/s; absent when the snapshot carries no
   * attribute data. */
  baseDemand?: number;
  pressure: number | null;
  demand: number | null;
  /** Engine-generic current-period values keyed by catalog variable id,
   * merged by CanvasView for the rail list. `null` = not reported. */
  resultValues?: Record<string, number | null>;
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
  /** Mean velocity (m/s) for the current reporting period; absent when the
   * engine's snapshot carries no attribute data. */
  velocity?: number;
  /** Flow in L/s for the current reporting period. `null` when no simulation has run. */
  flow?: number | null;
  /**
   * Link status from the simulation result (Hydra OUT-file codes):
   * 3 = Open, 2 = Closed, 4 = Active, 0 = XHead, 1 = TempClosed, 6 = XFcv, 7 = XPressure.
   * `null` when no simulation has run.
   */
  status?: number | null;
  /** Initial [STATUS] from the INP (pipes only): open, closed, or check valve. */
  initialStatus?: "open" | "closed" | "cv";
  /** Diameter in mm; absent when the engine's snapshot carries no
   * attribute data — never fabricate a 0 for it. */
  diameter?: number;
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
  /** Engine-generic current-period values keyed by catalog variable id,
   * merged by CanvasView for the rail list. `null` = not reported. */
  resultValues?: Record<string, number | null>;
}

export interface Pattern {
  id: string;
  /** Dimensionless multipliers [F₀, F₁, …, F_{L−1}]. */
  multipliers: number[];
}
