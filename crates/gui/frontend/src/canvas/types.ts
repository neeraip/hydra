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
