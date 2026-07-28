/* Project view configuration. Engine identity (label, pill, accent) is
   registry-driven — see hooks/engines.ts; nothing engine-specific may be
   hardcoded here. */

import {
  ChartBarSquareIcon,
  MapIcon,
  RectangleGroupIcon,
  TableCellsIcon,
} from "@heroicons/react/24/outline";
import type { ComponentType, SVGProps } from "react";

// ── App accent ───────────────────────────────────────────────────────────────

/** App-wide accent colour for generic chrome (buttons, highlights, toasts).
 * Currently equal to the WDS engine accent while Hydra ships one engine.
 * Engine-identity surfaces (project cards, pills, wizard, status bar) must
 * NOT use this — they resolve the project's engine via
 * `useEngines`/`engineByKey` instead. */
export const ACCENT = "#4a90d9" as const;

// ── Project view identifiers ─────────────────────────────────────────────────

/**
 * Top-level project views. Canvas is the primary workspace; Overview is the
 * landing screen when a project is first opened.
 */
export type ProjectView = "overview" | "canvas" | "editor" | "analysis";

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
];
