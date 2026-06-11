/* Project view configuration for the Water Distribution engine.
   Hydra is exclusively a water distribution simulator. */

import {
  ChartBarSquareIcon,
  MapIcon,
  RectangleGroupIcon,
  TableCellsIcon,
} from "@heroicons/react/24/outline";
import type { ComponentType, SVGProps } from "react";

// ── Identity constants ───────────────────────────────────────────────────────

/** Full display label shown in project cards and the new-project wizard. */
export const LABEL = "Water Distribution" as const;
/** Compact pill code shown on cards. */
export const PILL = "WD" as const;
/** Hex accent colour used for thumbnails / pill backgrounds. */
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
