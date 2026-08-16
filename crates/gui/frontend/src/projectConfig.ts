/* Project view configuration. Engine identity (label, pill, accent) is
   registry-driven — see hooks/engines.ts; nothing engine-specific may be
   hardcoded here. */

import {
  ChartBarSquareIcon,
  DocumentTextIcon,
  MapIcon,
  RectangleGroupIcon,
  TableCellsIcon,
} from "@heroicons/react/24/outline";
import type { ComponentType, SVGProps } from "react";

// ── App accent ───────────────────────────────────────────────────────────────

/** App-wide accent for generic chrome (buttons, highlights, toasts).
 *
 * The token, not a copy of its value. It used to hold the wds engine's hex
 * — the collision that made an engine's identity mark indistinguishable
 * from "this is selected" — and being a literal it also sat outside the
 * theme, so it stayed blue when the accent stopped being blue.
 *
 * Engine-identity surfaces (project cards, pills, wizard, status bar) must
 * NOT use this — they resolve the project's engine via
 * `useEngines`/`engineByKey` instead. */
export const ACCENT = "var(--accent)" as const;

// ── Project view identifiers ─────────────────────────────────────────────────

/**
 * Top-level project views. Canvas is the primary workspace; Overview is the
 * landing screen when a project is first opened.
 */
export type ProjectView =
  | "overview"
  | "canvas"
  | "editor"
  | "analysis"
  | "report";

type IconCmp = ComponentType<SVGProps<SVGSVGElement>>;

export interface ProjectViewSpec {
  id: ProjectView;
  label: string;
  icon: IconCmp;
  /** If `true`, this view is fully implemented. */
  ready?: boolean;
}

// ── Views ────────────────────────────────────────────────────────────────────

export const PROJECT_VIEWS: ProjectViewSpec[] = [
  { id: "overview", label: "Overview", icon: RectangleGroupIcon, ready: true },
  { id: "canvas", label: "Canvas", icon: MapIcon, ready: true },
  { id: "editor", label: "Editor", icon: TableCellsIcon, ready: true },
  { id: "analysis", label: "Results", icon: ChartBarSquareIcon, ready: true },
  { id: "report", label: "Report", icon: DocumentTextIcon, ready: true },
];

/**
 * The number key that jumps to each view.
 *
 * Beside the view list rather than in `App.tsx`, because two places read
 * it and they had drifted: the key handler routed ⌘4 to `analysis` while
 * the shortcut card called that view "Analysis", a name it stopped having
 * when it was relabelled "Results" here. A card is only ever read by
 * someone who does not already know the answer, so a wrong row is
 * believed.
 *
 * Every view in `PROJECT_VIEWS` has a key, in the order the activity bar
 * draws them. The Report view went without one for as long as it existed,
 * which nothing caught because the card listing these was written by hand
 * and simply stopped at four.
 */
export const VIEW_SHORTCUTS: Record<string, ProjectView> = {
  "1": "overview",
  "2": "canvas",
  "3": "editor",
  "4": "analysis",
  "5": "report",
};
