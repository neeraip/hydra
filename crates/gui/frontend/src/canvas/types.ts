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
 * A position the canvas reports back, and which space it is in.
 *
 * The canvas draws two kinds of model. A georeferenced one is drawn on a
 * basemap in WGS84, so a drop point arrives as longitude and latitude and
 * has to be inverse-projected before it can be stored. A local grid is
 * drawn orthographically at its own coordinates, so a drop point *is* the
 * stored value and projecting it would corrupt it.
 *
 * The tag exists because both are two numbers called x and y, and the only
 * thing distinguishing "4.89, 52.37" from "4890, 52370" is which renderer
 * produced it. Passing them untagged is how a plan view would silently
 * write degrees into a metre grid.
 */
export type CanvasPoint =
  /** Longitude, latitude. */
  | { space: "wgs84"; x: number; y: number }
  /** The model's own coordinates, as stored. */
  | { space: "source"; x: number; y: number };

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
