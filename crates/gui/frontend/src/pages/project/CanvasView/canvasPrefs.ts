/**
 * What the canvas remembers about a project, and how it comes back.
 *
 * Persisted under one JSON key per project — unlike the link-animation
 * toggle, which is deliberately global and lives elsewhere.
 *
 * Reading them back is the part worth having on its own. It is a dozen
 * decisions taken together, every one of them a fallback, and it ran inside
 * an effect where nothing could call it. That effect has already produced
 * one defect: preferences used to be applied only where a stored value was
 * present and valid, which left the *previous* project's setting in place
 * for any project that had never saved any — and the persist effect then
 * wrote it under the new project's key, making the bleed permanent.
 * `resolveCanvasPrefs` answers for every preference every time, which is
 * what stops that, and now says so somewhere a test can ask.
 */

import {
  type BasemapId,
  clampBasemapOpacity,
  isValidBasemapId,
} from "../../../canvas/Basemap";
import {
  type CanvasBackground,
  DEFAULT_CANVAS_BACKGROUND,
  readCanvasBackground,
} from "../../../canvas/canvasBackground";
import {
  LINK_VARIABLES,
  NODE_VARIABLES,
} from "../../../canvas/canvasVariables";
import type {
  GenericClassKey,
  GenericSelection,
} from "../../../canvas/GenericLegend";
import type { ScaleMode } from "../../../canvas/legend-primitives";
import { NODE_SCALE_DEFAULT } from "../../../canvas/nodeScale";
import { ASPECT_SLIDER_DEFAULT } from "../../../canvas/schematicAspect";
import type {
  LinkVariable,
  NodeVariable,
  ViewMode,
} from "../../../canvas/types";
import { clampSliderValue } from "../../../canvas/verticalSlider";

// ── Per-project canvas prefs ────────────────────────────────────────────────
// Persisted under one JSON key per project (unlike hydra2-link-animation,
// which is deliberately a global preference and stays untouched).
const canvasPrefsKey = (projectId: string) =>
  `hydra2-canvas-prefs:${projectId}`;

export interface CanvasPrefs {
  viewMode: ViewMode;
  /** Legacy id ("streets"/"light"/"dark"/"none", stored unchanged for
   * backwards compatibility) or `provider:{providerId}:{styleId}`. */
  basemap: BasemapId;
  /** Basemap dimming, 0–1. Missing in older prefs → defaults to 1. */
  basemapOpacity: number;
  nodeVar: NodeVariable;
  linkVar: LinkVariable;
  /** Schematic layout aspect slider position. Missing in older prefs →
   * defaults to the midpoint, i.e. the layout's native 120:80 spacing. */
  schematicAspect: number;
  /** Node-size slider position. Missing in older prefs → the default. */
  nodeScale: number;
  /** The ground under a canvas with no basemap. Missing → tracks the app
   * theme, which is what it did before it could be chosen. */
  canvasBackground: CanvasBackground;
  /**
   * The range the colour ramps span: the whole run, or the current step.
   *
   * Was three-valued, with `criteria` as a third answer. That merged two
   * questions — see `criteriaScale` — so a saved `criteria` migrates to
   * `run` plus the toggle. Prefs written before *that* merge, when this
   * was `colorMode` and `rangeMode`, are migrated on read as well.
   */
  scaleMode: ScaleMode;
  /**
   * Which element classes are coloured by their verdict rather than by
   * magnitude, when the variable they are showing has criteria.
   *
   * Per class rather than one switch for the map. Both engines band two
   * variables in different classes — pressure and velocity, velocity and
   * capacity — and "judge the pressures, show me velocity as a magnitude"
   * is a real reading that a single flag cannot express.
   *
   * Independent of the range above: a variable with no criteria, or a
   * class with this off, keeps its magnitude colouring on whichever range
   * is chosen.
   */
  criteriaScale: Record<GenericClassKey, boolean>;
  /** Whether the legend's ramp popover is showing. Persisted because it is
   * a working mode — "keep the full legend up while I read the map" —
   * rather than a menu the user reopens each time. */
  legendOpen: boolean;
  /**
   * Selected catalog variable per element class.
   *
   * The variables a reader chose are what the map *means* to them, so they
   * survive reopening the project like every other canvas choice. Only the
   * wds pair was persisted before (as `nodeVar`/`linkVar`, which its canvas
   * colours from); drainage lost its selection on every remount, and no
   * engine remembered its areal variable at all.
   *
   * Ids are stored raw and validated on use rather than on read: the
   * catalog they belong to depends on the run, and the legend already
   * falls back to the first variable when an id is not in it.
   */
  genericSelection: GenericSelection;
}

/**
 * Defaults for every persisted canvas preference.
 *
 * Shared by the initial state and by the project-switch restore: a project
 * with no saved prefs must be reset to these, not left holding whatever the
 * previously open project was showing.
 */
export const CANVAS_PREF_DEFAULTS: CanvasPrefs = {
  viewMode: "map",
  basemap: "streets",
  basemapOpacity: 1,
  nodeVar: "pressure",
  linkVar: "velocity",
  schematicAspect: ASPECT_SLIDER_DEFAULT,
  nodeScale: NODE_SCALE_DEFAULT,
  canvasBackground: DEFAULT_CANVAS_BACKGROUND,
  scaleMode: "run",
  criteriaScale: {
    point: false,
    polyline: false,
    region: false,
    surface: false,
  },
  legendOpen: false,
  genericSelection: { point: "", polyline: "", region: "", surface: "" },
};

// Allowlists so corrupt/stale localStorage can never inject invalid state.
// (Basemap ids are validated structurally via isValidBasemapId instead — the
// provider catalog is open-ended.)
const PREF_VIEW_MODES: readonly ViewMode[] = ["map", "schematic"];

/**
 * What a colour ramp is scaled against.
 *
 * `run` — the whole simulation's range. Colours mean the same thing at
 * every step, so scrubbing shows a quantity rising and falling. The cost is
 * that a step whose values occupy a sliver of the run's range is painted in
 * a sliver of the ramp, and its spatial pattern is invisible.
 *
 * `step` — the current period's own range. Every frame uses the full ramp,
 * so the pattern *within* a moment is as legible as it can be. The cost is
 * that colours no longer compare between steps: a bright node now and a
 * bright node later are each the highest of their own moment, not equal.
 *
 * Judging against the project's threshold bands is not a range at all —
 * it answers "is this acceptable?" rather than "how much?" — so it lives
 * in `criteriaScale` and combines with either of these.
 */
const PREF_SCALE_MODES: readonly ScaleMode[] = ["run", "step"];

/**
 * Read a persisted per-class variable selection, tolerating anything.
 *
 * Every field is optional and unvalidated against a catalog on purpose:
 * which variables exist depends on the run that produced the results, and
 * an id that is not in the current catalog is not corrupt — it is a
 * selection made against a different one. The legend falls back to the
 * catalog's first variable for those, which is the same thing it does for
 * a project that has never chosen.
 */
export function readGenericSelection(raw: unknown): GenericSelection {
  const empty = { point: "", polyline: "", region: "", surface: "" };
  if (typeof raw !== "object" || raw === null) return empty;
  const sel = (raw as { genericSelection?: unknown }).genericSelection;
  if (typeof sel !== "object" || sel === null) return empty;
  const s = sel as Record<string, unknown>;
  const str = (v: unknown) => (typeof v === "string" ? v : "");
  return {
    point: str(s.point),
    polyline: str(s.polyline),
    region: str(s.region),
    surface: str(s.surface),
  };
}

/**
 * Read a persisted range, migrating both earlier shapes.
 *
 * Two migrations sit on top of each other. The oldest prefs held
 * `colorMode` (relative | threshold) beside `rangeMode` (run | step);
 * those became one three-valued `scaleMode`, and that has now split again
 * into a range plus `criteriaScale`. A stored `criteria` therefore has no
 * range of its own to recover — it was the answer to the other question —
 * and resolves to `run`, which is what it always behaved as (nothing but
 * `step` ever rescaled).
 */
export function readScaleMode(raw: unknown): ScaleMode {
  if (typeof raw !== "object" || raw === null) return "run";
  const p = raw as Record<string, unknown>;
  if (typeof p.scaleMode === "string") {
    const v = p.scaleMode as ScaleMode;
    if (PREF_SCALE_MODES.includes(v)) return v;
    // A saved `criteria` falls through to the range it was drawn on.
  }
  // A saved `criteria` was the answer to the *other* question and carries
  // no range of its own; nothing but `step` ever rescaled, so it behaved
  // as `run` and still does.
  if (p.scaleMode === "criteria") return "run";
  // `colorMode` is deliberately not consulted here. In the oldest shape it
  // sat *beside* `rangeMode`, so "threshold" and "step" were both saved
  // and both meant — a combination the merge discarded and the split can
  // honour again.
  if (p.rangeMode === "step") return "step";
  return "run";
}

/**
 * Read which classes judge against criteria, from any earlier shape.
 *
 * Three shapes preceded this and every one of them said "all of them":
 * a `colorMode` of `threshold`, a three-valued `scaleMode` of `criteria`,
 * and the single boolean that briefly replaced it. A reader who had
 * turned judging on keeps it on for every class rather than finding their
 * canvas quietly back on magnitudes.
 */
export function readCriteriaScale(
  raw: unknown,
): Record<GenericClassKey, boolean> {
  // `surface` stays false in the legacy-boolean migration: no criteria
  // catalog judges surface variables, so nothing older can have meant it.
  const all = (on: boolean) => ({
    point: on,
    polyline: on,
    region: on,
    surface: false,
  });
  if (typeof raw !== "object" || raw === null) return all(false);
  const p = raw as Record<string, unknown>;
  const stored = p.criteriaScale;
  if (typeof stored === "boolean") return all(stored);
  if (typeof stored === "object" && stored !== null) {
    const s = stored as Record<string, unknown>;
    const flag = (k: string) => s[k] === true;
    return {
      point: flag("point"),
      polyline: flag("polyline"),
      region: flag("region"),
      surface: flag("surface"),
    };
  }
  return all(p.scaleMode === "criteria" || p.colorMode === "threshold");
}

const PREF_NODE_VARS: readonly NodeVariable[] = NODE_VARIABLES;
const PREF_LINK_VARS: readonly LinkVariable[] = LINK_VARIABLES;

export function readCanvasPrefs(
  projectId: string,
): Partial<CanvasPrefs> | null {
  try {
    const raw = localStorage.getItem(canvasPrefsKey(projectId));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<CanvasPrefs>;
    return typeof parsed === "object" && parsed !== null ? parsed : null;
  } catch {
    return null;
  }
}

/**
 * Every canvas preference, resolved from whatever was stored.
 *
 * Answers for all of them rather than only the ones the stored object
 * happened to carry — see this module's note for the bleed that rule exists
 * to stop. A project with nothing saved comes back as the defaults, which
 * is what a fresh install shows.
 *
 * Each is validated the way its own value can be: allowlists for the closed
 * sets, a structural check for basemap ids because the provider catalog is
 * open-ended, clamps for the sliders, and nothing at all for the variable
 * selection — an id outside the current catalog is not corrupt, it is a
 * choice made against a different run.
 */
export function resolveCanvasPrefs(
  stored: Partial<CanvasPrefs> | null,
): CanvasPrefs {
  const pick = <K extends keyof CanvasPrefs>(
    key: K,
    valid: (v: CanvasPrefs[K]) => boolean,
  ): CanvasPrefs[K] => {
    const v = stored?.[key];
    return v !== undefined && valid(v) ? v : CANVAS_PREF_DEFAULTS[key];
  };

  const nodeVar = pick("nodeVar", (v) => PREF_NODE_VARS.includes(v));
  const linkVar = pick("linkVar", (v) => PREF_LINK_VARS.includes(v));
  // Seeded from that legacy pair when no selection was saved — prefs
  // written before the legend became the single store have only those.
  const savedSelection = readGenericSelection(stored);

  return {
    viewMode: pick("viewMode", (v) => PREF_VIEW_MODES.includes(v)),
    basemap: pick("basemap", (v) => isValidBasemapId(v)),
    // The clamps map a missing or corrupt value to the default themselves,
    // so they need no `pick`.
    basemapOpacity: clampBasemapOpacity(stored?.basemapOpacity),
    schematicAspect: clampSliderValue(stored?.schematicAspect ?? Number.NaN),
    nodeScale: Number.isFinite(stored?.nodeScale)
      ? (stored?.nodeScale as number)
      : CANVAS_PREF_DEFAULTS.nodeScale,
    canvasBackground: readCanvasBackground(stored?.canvasBackground),
    // Reads the merged key, falling back to the pre-merge pair.
    scaleMode: readScaleMode(stored),
    criteriaScale: readCriteriaScale(stored),
    legendOpen: pick("legendOpen", (v) => typeof v === "boolean"),
    nodeVar,
    linkVar,
    genericSelection: {
      ...savedSelection,
      point: savedSelection.point || nodeVar,
      polyline: savedSelection.polyline || linkVar,
    },
  };
}

/** Persist a project's canvas preferences. Best-effort: a full or
 *  unavailable store loses a preference, not model data. */
export function writeCanvasPrefs(projectId: string, prefs: CanvasPrefs): void {
  try {
    localStorage.setItem(canvasPrefsKey(projectId), JSON.stringify(prefs));
  } catch {
    // Ignored, deliberately.
  }
}
