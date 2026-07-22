/**
 * Exhibit types + static UI config (themes, styles, scopes).
 */

import type { Link, Node } from "../types";

export type ExhibitTheme =
  | "pressure"
  | "velocity"
  | "pipe-age"
  | "fire-flow"
  | "calibration-rmse"
  | "demand";
export type ExhibitStyle = "choropleth" | "graduated" | "dot" | "heatmap";
export type ExhibitScope = "whole" | "selection" | "south-side" | "north-feed";

export interface ThemeSpec {
  id: ExhibitTheme;
  label: string;
  unit: string;
  stops: { v: number; color: string; label?: string }[];
  defaultNode: number;
  nodeValue: (n: Node) => number | null;
  linkValue: (l: Link) => number | null;
  narrative: string;
}
export interface ExhibitSpec {
  id: string;
  title: string;
  caption: string;
  theme: ExhibitTheme;
  style: ExhibitStyle;
  scope: ExhibitScope;
  showLegend: boolean;
  showScale: boolean;
  showNorth: boolean;
  callouts: { id: string; nodeId: string; text: string }[];
  sectionId: string | null;
}

export const STYLE_SPECS: { id: ExhibitStyle; label: string; desc: string }[] =
  [
    {
      id: "choropleth",
      label: "Choropleth",
      desc: "Solid color fill on nodes or pipes by class.",
    },
    {
      id: "graduated",
      label: "Graduated",
      desc: "Line/circle thickness scaled to value.",
    },
    {
      id: "dot",
      label: "Dot density",
      desc: "Proportional dots, best for demand and counts.",
    },
    {
      id: "heatmap",
      label: "Heatmap",
      desc: "Smooth radial blend, best for coverage themes.",
    },
  ];
export const SCOPE_SPECS: { id: ExhibitScope; label: string; desc: string }[] =
  [
    { id: "whole", label: "Whole network", desc: "All junctions and pipes." },
    {
      id: "selection",
      label: "Current selection",
      desc: "Items currently selected on the canvas.",
    },
    {
      id: "south-side",
      label: "South Side district",
      desc: "DMA covering rows 2 and 3.",
    },
    {
      id: "north-feed",
      label: "North feed",
      desc: "Pumped supply from reservoir into tank.",
    },
  ];

function _h(id: string, salt = 0): number {
  let x = salt;
  for (let i = 0; i < id.length; i++) x = ((x << 5) - x + id.charCodeAt(i)) | 0;
  return Math.abs(x);
}

export const THEMES: Record<ExhibitTheme, ThemeSpec> = {
  pressure: {
    id: "pressure",
    label: "Pressure",
    unit: "m",
    stops: [
      { v: 20, color: "#c94040", label: "< 24" },
      { v: 30, color: "#d4a017", label: "24–35" },
      { v: 40, color: "#3daf75", label: "35–45" },
      { v: 50, color: "#4a90d9", label: "> 45" },
    ],
    defaultNode: 35,
    nodeValue: (n) => n.pressure,
    linkValue: () => null,
    narrative: "Junction pressure at peak demand hour (HH 08:00).",
  },
  velocity: {
    id: "velocity",
    label: "Velocity",
    unit: "m/s",
    stops: [
      { v: 0.0, color: "#666", label: "stagnant" },
      { v: 0.3, color: "#4a90d9", label: "0.3" },
      { v: 1.0, color: "#3daf75", label: "1.0" },
      { v: 1.8, color: "#d4a017", label: "1.8" },
      { v: 2.5, color: "#c94040", label: "> 2.5" },
    ],
    defaultNode: 0,
    nodeValue: () => null,
    linkValue: (l) => l.velocity,
    narrative:
      "Pipe velocity at peak demand. Stagnant pipes flagged for flushing.",
  },
  "pipe-age": {
    id: "pipe-age",
    label: "Pipe age",
    unit: "yrs",
    stops: [
      { v: 0, color: "#3daf75", label: "< 20" },
      { v: 30, color: "#d4a017", label: "20–50" },
      { v: 60, color: "#c94040", label: "> 60" },
    ],
    defaultNode: 0,
    nodeValue: () => null,
    linkValue: (l) => (_h(l.id, 17) % 80) + 5,
    narrative:
      "Estimated pipe age from asset register. Highlights candidates for renewal.",
  },
  "fire-flow": {
    id: "fire-flow",
    label: "Fire-flow availability",
    unit: "L/s",
    stops: [
      { v: 0, color: "#c94040", label: "< 15" },
      { v: 25, color: "#d4a017", label: "15–30" },
      { v: 35, color: "#3daf75", label: "> 30" },
    ],
    defaultNode: 25,
    nodeValue: (n) => 8 + (_h(n.id, 91) % 35),
    linkValue: () => null,
    narrative: "Available fire-flow at 14 m residual pressure per AS 2419.1.",
  },
  "calibration-rmse": {
    id: "calibration-rmse",
    label: "Calibration RMSE",
    unit: "m",
    stops: [
      { v: 0, color: "#3daf75", label: "< 1" },
      { v: 2, color: "#d4a017", label: "1–3" },
      { v: 4, color: "#c94040", label: "> 3" },
    ],
    defaultNode: 0,
    nodeValue: (n) => (_h(n.id, 7) % 50) / 10,
    linkValue: () => null,
    narrative: "Per-node RMSE between simulated and observed pressure traces.",
  },
  demand: {
    id: "demand",
    label: "Demand",
    unit: "L/s",
    stops: [
      { v: 0, color: "#1a3a5c", label: "0" },
      { v: 3, color: "#4a90d9", label: "3" },
      { v: 6, color: "#9bc8ec", label: "6+" },
    ],
    defaultNode: 0,
    nodeValue: (n) => n.demand ?? 0,
    linkValue: () => null,
    narrative:
      "Junction base demand. Larger circles = higher demand allocation.",
  },
};

export function defaultExhibit(theme: ExhibitTheme): ExhibitSpec {
  const t = THEMES[theme];
  return {
    id: `EX-${Date.now().toString(36)}`,
    title: `${t.label}: Peak Hour`,
    caption: t.narrative,
    theme,
    style:
      theme === "demand"
        ? "dot"
        : theme === "velocity" || theme === "pipe-age"
          ? "graduated"
          : "choropleth",
    scope: "whole",
    showLegend: true,
    showScale: true,
    showNorth: true,
    callouts: [],
    sectionId: null,
  };
}
