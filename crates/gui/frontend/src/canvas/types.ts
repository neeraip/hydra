import type { GenericVariable } from "../hooks/results";

export type NodeVariable = "pressure" | "head" | "demand" | "quality";
export type LinkVariable =
  | "flow"
  | "velocity"
  | "status"
  | "headloss"
  | "quality";
export type ResultsTab = "summary" | "charts" | "balance" | "analytics";
export type ViewMode = "map" | "schematic";
export type CanvasTool =
  | "select"
  | "measure"
  | "edit"
  | "add-node"
  | "add-link";

export interface ClickPoint {
  x: number;
  y: number;
}

/**
 * One engine-described result channel, ready to colour a canvas layer:
 * the selected catalog variable (label, unit, ramp hint, per-run range)
 * plus one value per element in canvas order. `NaN` marks an element the
 * results file does not report. `values` is `null` while a period fetch is
 * in flight.
 */
export interface GenericChannel {
  variable: GenericVariable;
  values: Float32Array | null;
}

/**
 * Engine-generic canvas colouring, one channel per element class. Built by
 * `CanvasView` from `ResultMeta.generic` + the generic period payload for
 * engines whose results are variable-keyed (uds); `MapCanvas` renders it
 * with zero engine knowledge.
 */
export interface GenericCanvasResults {
  node: GenericChannel | null;
  link: GenericChannel | null;
  region: GenericChannel | null;
}
