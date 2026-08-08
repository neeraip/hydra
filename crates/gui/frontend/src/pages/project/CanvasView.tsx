import { XMarkIcon } from "@heroicons/react/16/solid";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useActiveProject, useAppState, useSimulation } from "../../AppContext";
import {
  type BasemapId,
  clampBasemapOpacity,
  isValidBasemapId,
} from "../../canvas/Basemap";
import {
  type CanvasBackground,
  canvasBackgroundStyle,
  DEFAULT_CANVAS_BACKGROUND,
  readCanvasBackground,
} from "../../canvas/canvasBackground";
import {
  LINK_VARIABLES,
  linkVariableFor,
  NODE_VARIABLES,
  nodeVariableFor,
} from "../../canvas/canvasVariables";
import {
  haversineMeters,
  LOCAL_CRS,
  wgs84ToSourceCrs,
} from "../../canvas/coords";
import {
  type GenericClassKey,
  GenericLegend,
  type GenericSelection,
} from "../../canvas/GenericLegend";
import type { ScaleMode } from "../../canvas/legend-primitives";
import { MapCanvas } from "../../canvas/MapCanvas";
import { wdsBandColors } from "../../canvas/MapCanvas/colorUtils";
import type { MeasurePoint } from "../../canvas/measureSnap";
import { NODE_SCALE_DEFAULT } from "../../canvas/nodeScale";
import { usePublishCurrentPeriod } from "../../canvas/period-context";
import { stepIntervalMs } from "../../canvas/playback";
import { withQualityAvailability } from "../../canvas/qualityAvailability";
import {
  ASPECT_SLIDER_DEFAULT,
  aspectScales,
  clampSliderValue,
} from "../../canvas/schematicAspect";
import type { InspectorView } from "../../canvas/selection-context";
import { useCanvasSelection } from "../../canvas/selection-context";
import { Timeline } from "../../canvas/Timeline";
import type {
  CanvasTool,
  GenericCanvasResults,
  LinkVariable,
  NodeVariable,
  ViewMode,
} from "../../canvas/types";
import { CreateLinkModal } from "../../components/modals/CreateLinkModal";
import {
  CreateNodeModal,
  type NodeCreatePayload,
} from "../../components/modals/CreateNodeModal";
import { DeleteConfirmModal } from "../../components/modals/DeleteConfirmModal";
import { CriteriaEditor } from "../../components/panels/CriteriaEditor";
import {
  LinkInspector,
  NodeInspector,
  RegionInspector,
} from "../../components/panels/ElementInspector";
import { engineComponents } from "../../engine/registry";
import {
  createLink,
  createNode,
  deleteElement,
  type GenericPeriodValues,
  type GenericQuantity,
  type GenericVariable,
  getGenericPeriodValues,
  getPeriodResults,
  type PeriodResults,
  patchNodePosition,
  resultsPath,
  saveProjectOnDisk,
  useElementKinds,
  useInletCouplings,
  useLinks,
  useNodes,
  useProjectCriteria,
  useRegions,
  useSimParams,
} from "../../hooks";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";
import {
  pushUndoEntry,
  recreateSpecsForDelete,
  stackKey,
} from "../../hooks/undoStack";
import { useElementRename } from "../../hooks/useElementRename";
import { useReducedMotion } from "../../hooks/useReducedMotion";
import type { Link, Node, Region } from "../../types/network";
import { type Quantity, toDisplay, useUnitSystem } from "../../units";
import { CanvasErrorBoundary } from "./CanvasView/CanvasErrorBoundary";
import { CanvasToolbar } from "./CanvasView/CanvasToolbar";
import { InvalidCrsOverlay } from "./CanvasView/InvalidCrsOverlay";
import { NodeSizeSlider } from "./CanvasView/NodeSizeSlider";
import { SchematicAspectSlider } from "./CanvasView/SchematicAspectSlider";
import { useCrsReprojection } from "./CanvasView/useCrsReprojection";
import { ViewportControls } from "./CanvasView/ViewportControls";
import { linkResultsAt, nodeResultsAt } from "./mergeResults";
import {
  shouldZoomOnFollow,
  type ViewportCause,
  viewportIsUserOwned,
} from "./viewportCause";

const NODE_KIND_PREFIX: Record<string, string> = {
  junction: "J",
  reservoir: "R",
  tank: "T",
};

// ── Per-project canvas prefs ────────────────────────────────────────────────
// Persisted under one JSON key per project (unlike hydra2-link-animation,
// which is deliberately a global preference and stays untouched).
const canvasPrefsKey = (projectId: string) =>
  `hydra2-canvas-prefs:${projectId}`;

interface CanvasPrefs {
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
   * What the colour ramps are scaled against: the whole run, the current
   * step, or the project's criteria bands.
   *
   * Supersedes the old `colorMode`, whose "relative"/"threshold" pair asked
   * the same question with two of the three answers. Prefs written before
   * the merge are migrated on read.
   */
  scaleMode: ScaleMode;
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
const CANVAS_PREF_DEFAULTS: CanvasPrefs = {
  viewMode: "map",
  basemap: "streets",
  basemapOpacity: 1,
  nodeVar: "pressure",
  linkVar: "velocity",
  schematicAspect: ASPECT_SLIDER_DEFAULT,
  nodeScale: NODE_SCALE_DEFAULT,
  canvasBackground: DEFAULT_CANVAS_BACKGROUND,
  scaleMode: "run",
  legendOpen: false,
  genericSelection: { point: "", polyline: "", region: "" },
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
 * `criteria` — the project's threshold bands, ignoring the data range
 * entirely. Colours then answer "is this acceptable?" rather than "how
 * much?", and stay fixed while the model changes around them.
 */
const PREF_SCALE_MODES: readonly ScaleMode[] = ["run", "step", "criteria"];

/**
 * Read a persisted scale mode, migrating prefs written before the merge.
 *
 * `colorMode` asked "relative or threshold?" and `rangeMode` asked "whole
 * run or this step?" — the same question with two of the three answers, so
 * a saved "threshold" becomes `criteria` and a saved "relative" defers to
 * whatever range mode was stored alongside it.
 */
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
/**
 * What "Clear view" would currently dismiss.
 *
 * Split out so the button can be disabled when it would do nothing, and so
 * the two — what it clears, and whether it is offered — are derived from
 * one description instead of two lists that can disagree.
 */
export interface ClearableView {
  rail: boolean;
  selection: boolean;
  legend: boolean;
  basemapMenu: boolean;
  tool: boolean;
  measurements: boolean;
}

export function clearableCountOf(c: ClearableView): number {
  return Object.values(c).filter(Boolean).length;
}

/**
 * What the view button does next.
 *
 * It used to sit disabled once the view was clear, which is a control that
 * spends a permanent slot on the toolbar and is dead in the state it
 * created. The same press now brings the panels back, so the button is a
 * way in and out of an uncluttered map rather than a one-way trip.
 *
 * `restore` opens only what a reader can meaningfully be given back: the
 * rail and the legend. The rest of `ClearableView` has no inverse — there
 * is no selection to restore, no measurement to recreate, and reopening a
 * basemap dropdown nobody asked for would be an interruption rather than a
 * restoration.
 */
export type ViewAction = "clear" | "restore";

export function viewButtonAction(c: ClearableView): ViewAction {
  return clearableCountOf(c) === 0 ? "restore" : "clear";
}

/**
 * Whether a change in what covers the map should also re-fit the network.
 *
 * "Fitted" is a state, not a past action. Fit frames the network against
 * the *visible* map, so the moment a panel closes the viewport grows and
 * the framing Fit produced is no longer a fit — same camera, more room,
 * network small and off-centre. Re-fitting restores the invariant rather
 * than inventing a move.
 *
 * It is conditional because the two failure directions are not
 * symmetrical. Failing to re-fit costs a small convenience. Re-fitting a
 * camera the user positioned themselves discards deliberate work and
 * cannot be undone. So this answers yes only when the app owns the current
 * framing, and anything ambiguous must mark the viewport as the user's.
 *
 * `occlusionChanged` gates out the no-op case: closing something that
 * never covered the map (a dropdown, the ramp popover) leaves the fit
 * exactly as it was, and a camera animation with no visible cause reads
 * as a glitch.
 */
export function shouldRefitAfterOcclusionChange(
  viewportIsAppOwned: boolean,
  occlusionChanged: boolean,
): boolean {
  return viewportIsAppOwned && occlusionChanged;
}

export function readGenericSelection(raw: unknown): GenericSelection {
  const empty = { point: "", polyline: "", region: "" };
  if (typeof raw !== "object" || raw === null) return empty;
  const sel = (raw as { genericSelection?: unknown }).genericSelection;
  if (typeof sel !== "object" || sel === null) return empty;
  const s = sel as Record<string, unknown>;
  const str = (v: unknown) => (typeof v === "string" ? v : "");
  return {
    point: str(s.point),
    polyline: str(s.polyline),
    region: str(s.region),
  };
}

export function readScaleMode(raw: unknown): ScaleMode {
  if (typeof raw !== "object" || raw === null) return "run";
  const p = raw as Record<string, unknown>;
  if (typeof p.scaleMode === "string") {
    const v = p.scaleMode as ScaleMode;
    if (PREF_SCALE_MODES.includes(v)) return v;
  }
  if (p.colorMode === "threshold") return "criteria";
  if (p.rangeMode === "step") return "step";
  return "run";
}

/**
 * The range a ramp should use for one period's values.
 *
 * Falls back to the run range when the period's own span is a sliver of it.
 * A field that is essentially uniform — everything dry before the storm
 * arrives — has a span made of floating-point dust, and autoscaling to it
 * paints that dust across the whole ramp. Noise then looks exactly like
 * signal, which is worse than a flat frame.
 */
export function periodRange(
  values: Float32Array | null,
  runMin: number,
  runMax: number,
): { min: number; max: number } {
  if (!values || values.length === 0) return { min: runMin, max: runMax };
  let min = Number.POSITIVE_INFINITY;
  let max = Number.NEGATIVE_INFINITY;
  for (const v of values) {
    if (!Number.isFinite(v)) continue;
    if (v < min) min = v;
    if (v > max) max = v;
  }
  if (!Number.isFinite(min) || !Number.isFinite(max)) {
    return { min: runMin, max: runMax };
  }
  const runSpan = Math.abs(runMax - runMin);
  if (runSpan > 0 && Math.abs(max - min) < runSpan * 0.01) {
    return { min: runMin, max: runMax };
  }
  return { min, max };
}

/** Stable empties, so suppressing unplaceable geometry does not hand the
 * canvas a fresh array identity on every render. */
const EMPTY_NODES: Node[] = [];
const EMPTY_LINKS: Link[] = [];
const EMPTY_REGIONS: Region[] = [];

/** Label, engineering symbol and quantity for each fixed wds variable —
 * the frontend's own table, because these are not engine-catalog variables
 * and nothing serves descriptors for them. Symbols follow the same notation
 * the catalog engines use: p pressure, H head, q demand, Q flow, v velocity,
 * hL headloss, C concentration. */

/**
 * The labels a categorical variable's states go by, keyed by stored value.
 *
 * A categorical variable's values are codes, not measurements: link status
 * 3 means "Open". The engine publishes those states in the variable's own
 * ramp hint, so this reads them across rather than keeping a table of its
 * own — the contract's note on `Categorical` says exactly why, and the
 * frontend has already once shipped a private copy that drifted.
 */
function categoryLabels(
  ramp: GenericVariable["ramp"],
): Readonly<Record<number, { label: string; severity?: string }>> | undefined {
  if (ramp?.type !== "categorical") return undefined;
  const out: Record<number, { label: string; severity?: string }> = {};
  for (const item of ramp.items) {
    out[item.value] = { label: item.label, severity: item.severity };
  }
  return out;
}
/** How many result variables ride along on each rail element. The rail
 * shows one; the rest are there for the GeoJSON export. */
const RAIL_RESULT_COLUMNS = 3;

/** Catalog variables for one class, the legend's choice first.
 *
 * The rail shows the first column, so leading with the selected variable
 * is what makes the list follow the map. The rest ride along for the
 * GeoJSON export, which takes whatever is merged. Each column keeps the
 * index of its own values array, because reordering the columns must not
 * reorder what they read. */
export const railColumns = (
  vars: GenericVariable[],
  selected: string,
): Array<{
  key: string;
  label: string;
  symbol?: string;
  quantity?: GenericQuantity;
  codes?: Readonly<Record<number, { label: string; severity?: string }>>;
  range?: readonly [number, number];
  at: number;
}> => {
  // No catalog, no columns. `Math.max(0, -1)` below would otherwise turn
  // "not found" into index 0 and read it out of an empty array.
  if (vars.length === 0) return [];
  const chosen = Math.max(
    0,
    vars.findIndex((v) => v.id === selected),
  );
  const order = [
    chosen,
    ...vars.map((_, i) => i).filter((i) => i !== chosen),
  ].slice(0, RAIL_RESULT_COLUMNS);
  return order.map((i) => ({
    key: vars[i].id,
    label: vars[i].label,
    symbol: vars[i].symbol,
    quantity: vars[i].quantity,
    codes: categoryLabels(vars[i].ramp),
    range: [vars[i].min, vars[i].max] as const,
    at: i,
  }));
};

const WDS_NODE_VARS: Record<
  NodeVariable,
  { label: string; symbol: string; unit?: Quantity }
> = {
  pressure: { label: "Pressure", symbol: "p", unit: "pressure" },
  head: { label: "Head", symbol: "H", unit: "elevation" },
  demand: { label: "Demand", symbol: "q", unit: "demand" },
  quality: { label: "Quality", symbol: "C" },
};

const WDS_LINK_VARS: Record<
  LinkVariable,
  {
    label: string;
    symbol: string;
    unit?: Quantity;
  }
> = {
  flow: { label: "Flow", symbol: "Q", unit: "flow" },
  velocity: { label: "Velocity", symbol: "v", unit: "velocity" },
  // `headloss`, matching the catalog this same variable is served under
  // once a run exists. It was `elevation`, a third answer agreeing with
  // neither — harmless only because this table is the pre-run fallback and
  // has no values to convert, which is exactly how it went unnoticed.
  //
  // Both this and the catalog still describe pumps and valves as m/km when
  // the file stores their head loss as a total; that is inherent to the
  // .out convention and is not something a label can fix here.
  headloss: { label: "Headloss", symbol: "hL", unit: "headloss" },
  status: { label: "Status", symbol: "St" },
  quality: { label: "Quality", symbol: "C" },
};

const PREF_NODE_VARS: readonly NodeVariable[] = NODE_VARIABLES;
const PREF_LINK_VARS: readonly LinkVariable[] = LINK_VARIABLES;

/**
 * The criteria editor's box on the canvas.
 *
 * Top-left under the toolbar, clear of the legend it is opened from — a
 * panel that covered the legend would hide the bands whose effect you came
 * to watch. Scrolls rather than grows, so a short window cannot push the
 * fields off the bottom.
 */
const CRITERIA_PANEL_STYLE: React.CSSProperties = {
  position: "absolute",
  top: 64,
  left: "calc(var(--rail-effective-w, 0px) + 16px)",
  zIndex: 31,
  width: 520,
  maxWidth: "calc(100% - 32px)",
  maxHeight: "calc(100% - 150px)",
  overflowY: "auto",
  padding: 14,
  borderRadius: 10,
  transition: "left var(--rail-transition)",
};

function readCanvasPrefs(projectId: string): Partial<CanvasPrefs> | null {
  try {
    const raw = localStorage.getItem(canvasPrefsKey(projectId));
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<CanvasPrefs>;
    return typeof parsed === "object" && parsed !== null ? parsed : null;
  } catch {
    return null;
  }
}

export function CanvasView({ isActive = true }: { isActive?: boolean }) {
  const {
    activeScenarioId,
    openBasemapProvidersModal,
    openCrsModal,
    setProjectView,
    focusInEditor,
    projectView,
    railOpen,
    toggleRail,
    commandPaletteOpen,
    showToast,
  } = useAppState();
  const { project, engine } = useActiveProject();
  // Editing affordances exist only for engines whose model this GUI edits;
  // for read-only engines the tools hide rather than refuse per gesture.
  const { modelEditable, criteriaVariables, animatedVariables } =
    engineComponents(engine?.key);
  const { markEdited } = useNetworkVersion();
  const renameElementFlow = useElementRename();
  const simParams = useSimParams(project?.id);
  const {
    selectedNodeId,
    selectedLinkId,
    inspectorView,
    selectNode,
    selectLink,
    selectedRegionId,
    selectRegion,
    setInspectorView,
    setSelectedNodeId,
    setSelectedLinkId,
    clearSelection,
    setSimData,
    setZoomCallbacks,
  } = useCanvasSelection();
  const [activeTool, setActiveTool] = useState<CanvasTool>("select");
  useEffect(() => {
    if (
      !modelEditable &&
      (activeTool === "edit" ||
        activeTool === "add-node" ||
        activeTool === "add-link")
    ) {
      setActiveTool("select");
    }
  }, [modelEditable, activeTool]);
  const [currentHour, setCurrentHour] = useState(0);
  // A scrub position belongs to the run it was scrubbed in. Carrying it to
  // another project pointed at a period that project may not even have, and
  // silently answered "what is happening now" with someone else's clock.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the project id is the reset trigger.
  useEffect(() => {
    setCurrentHour(0);
  }, [project?.id]);
  // ── Link animation (Flow/Velocity pulse) — user toggle, persisted, and
  // forced off entirely while the "Reduce motion" accessibility setting is on.
  const [linkAnimation, setLinkAnimationRaw] = useState(
    () => localStorage.getItem("hydra2-link-animation") !== "false",
  );
  const setLinkAnimation = useCallback((v: boolean) => {
    setLinkAnimationRaw(v);
    localStorage.setItem("hydra2-link-animation", String(v));
  }, []);
  const reducedMotion = useReducedMotion();
  const animateLinks = linkAnimation && !reducedMotion;
  const [showBasemapDropdown, setShowBasemapDropdown] = useState(false);
  // ── Pending delete confirmation ───────────────────────────────────────────
  const [pendingDelete, setPendingDelete] = useState<{
    kind: string;
    id: string;
  } | null>(null);
  // ── Pending node / link creation ─────────────────────────────────────────
  const [pendingCreateNode, setPendingCreateNode] = useState<{
    lng: number;
    lat: number;
  } | null>(null);
  const [pendingCreateLink, setPendingCreateLink] = useState<{
    fromId: string;
    toId: string;
  } | null>(null);
  // ── Fly-to trigger ───────────────────────────────────────────────────────
  const [flyToState, setFlyToState] = useState<{
    nodeId: string | null;
    linkId: string | null;
    regionId: string | null;
    key: number;
  }>({ nodeId: null, linkId: null, regionId: null, key: 0 });

  // Flying to an element frames the map as deliberately as dragging it, and
  // it is triggered from five places. Marking ownership here — where every
  // one of them converges on the key bump — beats a fifth call site to
  // remember, which is exactly how such a rule comes to be applied in four
  // places out of five.
  useEffect(() => {
    if (flyToState.key === 0) return;
    lastViewportCauseRef.current = "feature";
  }, [flyToState.key]);

  /**
   * Follow a relationship named in the inspector to the element it names.
   *
   * Distinct from selecting, which is what a click on the canvas or a row
   * in the network list does — those already have the element in view, and
   * reframing them would be taking a camera the user is holding. Following
   * is navigation: the element is somewhere else by definition, and the
   * card naming it is the only thing on screen that knows where.
   *
   * Whether that reframes is `shouldZoomOnFollow`'s call, not this
   * function's. Note the fly-to marks the cause `"feature"` on its way
   * through, so following again keeps framing — the chain sustains itself
   * until the user pans.
   */
  const followElement = useCallback(
    (kind: "node" | "link" | "region", id: string) => {
      if (kind === "node") selectNode(id);
      else if (kind === "link") selectLink(id);
      else selectRegion(id);

      if (!shouldZoomOnFollow(lastViewportCauseRef.current)) return;
      setFlyToState((prev) => ({
        nodeId: kind === "node" ? id : null,
        linkId: kind === "link" ? id : null,
        regionId: kind === "region" ? id : null,
        key: prev.key + 1,
      }));
    },
    [selectNode, selectLink, selectRegion],
  );

  // Register zoom callbacks into the selection context so siblings (e.g. the
  // rail's network list) can trigger canvas fly-to without prop drilling.
  useEffect(() => {
    setZoomCallbacks(
      (id) =>
        setFlyToState((s) => ({
          nodeId: id,
          linkId: null,
          regionId: null,
          key: s.key + 1,
        })),
      (id) =>
        setFlyToState((s) => ({
          nodeId: null,
          linkId: id,
          regionId: null,
          key: s.key + 1,
        })),
      (id) =>
        setFlyToState((s) => ({
          nodeId: null,
          linkId: null,
          regionId: id,
          key: s.key + 1,
        })),
    );
  }, [setZoomCallbacks]);
  // ── Colour scale mode and per-variable thresholds ─────────────────────────
  const [scaleMode, setScaleMode] = useState<ScaleMode>(
    CANVAS_PREF_DEFAULTS.scaleMode,
  );
  const [legendOpen, setLegendOpen] = useState(CANVAS_PREF_DEFAULTS.legendOpen);
  const [genericSelection, setGenericSelection] = useState<GenericSelection>(
    CANVAS_PREF_DEFAULTS.genericSelection,
  );
  // Derived, not stored. See `canvasVariables`: holding these separately
  // from the legend's selection is what let the two name different
  // variables, with nothing on screen to say which one to believe.
  const nodeVar = useMemo(
    () => nodeVariableFor(genericSelection.point, CANVAS_PREF_DEFAULTS.nodeVar),
    [genericSelection.point],
  );
  const linkVar = useMemo(
    () =>
      linkVariableFor(genericSelection.polyline, CANVAS_PREF_DEFAULTS.linkVar),
    [genericSelection.polyline],
  );
  const unitSystem = useUnitSystem();
  // Threshold bands come from the project's criteria file. Previously they
  // were component state, so velocity and flow carried across project
  // switches — the canvas coloured one network against another's bands.
  const {
    criteria,
    setCriteria,
    saved: criteriaSaved,
  } = useProjectCriteria(project?.id ?? null);
  /**
   * Whether the criteria editor is open over the canvas.
   *
   * Over the canvas rather than on the Analysis page that also edits them,
   * because the reason to reach criteria from here is to move a band and
   * watch the map recolour under it. Sending the reader to another tab
   * makes that a round trip they have to remember the answer across.
   *
   * The same component and the same store as Analysis — two editors for one
   * value is how the halves of a setting come to disagree.
   */
  const [criteriaOpen, setCriteriaOpen] = useState(false);
  // Read by the shortcut handler, which is bound once and must not be
  // rebound on every open and close.
  const criteriaOpenRef = useRef(false);
  criteriaOpenRef.current = criteriaOpen;
  const thresholds = useMemo(
    () => ({
      pressure: criteria.pressure,
      velocity: criteria.velocity,
      flow: criteria.flow,
    }),
    [criteria],
  );
  // Seed pressure thresholds from SimulationOptions, but only for a project
  // that has never had criteria saved. Seeding unconditionally would discard
  // bands the user deliberately set every time the project loaded.
  const seededPressureFor = useRef<string | null>(null);
  useEffect(() => {
    const id = project?.id;
    // `saved === null` means the fetch is still in flight; seeding then would
    // treat "not yet known" as "none saved" and overwrite real criteria.
    if (!id || !simParams || criteriaSaved !== false) return;
    if (seededPressureFor.current === id) return;
    seededPressureFor.current = id;
    const min = simParams.pdaMinPressure;
    const req =
      simParams.pdaRequiredPressure > min
        ? simParams.pdaRequiredPressure
        : min + 11;
    setCriteria({
      ...criteria,
      pressure: { low: min, required: req, high: req + 10 },
    });
  }, [simParams, project?.id, criteriaSaved, criteria, setCriteria]);
  // ── View mode (Map vs Schematic) and basemap style ───────────────────
  // "none" is a *map* basemap (geographic layout, no tiles), distinct from
  // schematic mode (idealised orthogonal layout).
  const [viewMode, setViewMode] = useState<ViewMode>(
    CANVAS_PREF_DEFAULTS.viewMode,
  );
  // A model on a local drawing grid is not georeferenced, so there is no
  // basemap to put it on. It still has real geometry, so the canvas renders
  // it orthographically at its true coordinates rather than pretending the
  // numbers are longitude and latitude — which crashed MapLibre outright
  // ("Invalid LngLat latitude value") the moment anything flew to a feature.
  const localGrid = project?.sourceCrs === LOCAL_CRS;
  // What each kind does in the network, for the canvas's at-rest palette.
  // The engine declares it (spec §4.3); the canvas never names a kind.
  const elementKinds = useElementKinds(project?.engine);
  const kindRoles = useMemo(() => {
    const m = new Map<string, string>();
    for (const k of elementKinds) if (k.role) m.set(k.id, k.role);
    return m;
  }, [elementKinds]);

  // Tools that need the geographic renderer's pointer handling — dragging a
  // node, placing one, measuring between two points. A plan view has real
  // coordinates but draws them through the orthographic path, which has no
  // handlers for any of that.
  const geographic = viewMode === "map" && !localGrid;
  const [basemap, setBasemap] = useState<BasemapId>(
    CANVAS_PREF_DEFAULTS.basemap,
  );
  const [basemapOpacity, setBasemapOpacity] = useState(
    CANVAS_PREF_DEFAULTS.basemapOpacity,
  );
  // Schematic-only layout aspect. Persisted per project so a network tuned to
  // be readable stays that way on reopen; the midpoint default means a project
  // that never touched it is laid out exactly as before.
  const [schematicAspect, setSchematicAspect] = useState(
    CANVAS_PREF_DEFAULTS.schematicAspect,
  );
  const [nodeScale, setNodeScale] = useState(CANVAS_PREF_DEFAULTS.nodeScale);
  const [canvasBackground, setCanvasBackground] = useState<CanvasBackground>(
    CANVAS_PREF_DEFAULTS.canvasBackground,
  );
  // Memoised: MapCanvas keys its layout cache off this, and a fresh object per
  // render would re-lay out the whole network every render.
  const schematicScale = useMemo(
    () => aspectScales(schematicAspect),
    [schematicAspect],
  );

  // ── Per-project canvas prefs: restore on project switch, persist on change.
  // `prefsLoadedFor` gates persisting so the write effect (which also re-runs
  // on project switch) can never store the previous project's values under
  // the new project's key before the restore has been applied.
  const [prefsLoadedFor, setPrefsLoadedFor] = useState<string | null>(null);
  useEffect(() => {
    const id = project?.id;
    if (!id) return;
    const prefs = readCanvasPrefs(id);
    // Every preference is assigned unconditionally, falling back to the
    // shared default. Applying a value only when the stored one is present
    // and valid left the previous project's setting in place for any project
    // that had never saved prefs — and the persist effect below then wrote it
    // under the new project's key, making the bleed permanent.
    const pick = <K extends keyof CanvasPrefs>(
      key: K,
      valid: (v: CanvasPrefs[K]) => boolean,
    ): CanvasPrefs[K] => {
      const v = prefs?.[key];
      return v !== undefined && valid(v) ? v : CANVAS_PREF_DEFAULTS[key];
    };
    setViewMode(pick("viewMode", (v) => PREF_VIEW_MODES.includes(v)));
    setBasemap(pick("basemap", (v) => isValidBasemapId(v)));
    // Clamp already maps missing/corrupt values to the default.
    setBasemapOpacity(clampBasemapOpacity(prefs?.basemapOpacity));

    // Reads the merged key, falling back to the pre-merge pair.
    setScaleMode(readScaleMode(prefs));
    setLegendOpen(pick("legendOpen", (v) => typeof v === "boolean"));
    // Seeded from the legacy pair when no selection was saved — prefs
    // written before the legend became the single store have only those.
    const savedSelection = readGenericSelection(prefs);
    setGenericSelection({
      ...savedSelection,
      point:
        savedSelection.point ||
        pick("nodeVar", (v) => PREF_NODE_VARS.includes(v)),
      polyline:
        savedSelection.polyline ||
        pick("linkVar", (v) => PREF_LINK_VARS.includes(v)),
    });
    // Clamp already maps missing/corrupt values to the default.
    setSchematicAspect(clampSliderValue(prefs?.schematicAspect ?? Number.NaN));
    setNodeScale(
      Number.isFinite(prefs?.nodeScale)
        ? (prefs?.nodeScale as number)
        : CANVAS_PREF_DEFAULTS.nodeScale,
    );
    // Coerces a missing or corrupt value to the default itself.
    setCanvasBackground(readCanvasBackground(prefs?.canvasBackground));
    setPrefsLoadedFor(id);
  }, [project?.id]);
  // Cold-load gate: until the project row has arrived (an async fetch — it
  // carries sourceCrs) AND its persisted canvas prefs have been applied, the
  // defaults above ("streets" basemap, WGS84 CRS) are placeholders. Mounting
  // MapLibre with them paints the wrong basemap for a beat and can flash the
  // invalid-CRS alert against a projected network, so the map and the alert
  // hold back until this is true (the canvas background shows meanwhile).
  const prefsReady = project != null && prefsLoadedFor === project.id;
  useEffect(() => {
    const id = project?.id;
    if (!id || prefsLoadedFor !== id) return;
    const prefs: CanvasPrefs = {
      viewMode,
      basemap,
      basemapOpacity,
      nodeVar,
      linkVar,
      schematicAspect,
      nodeScale,
      canvasBackground,
      scaleMode,
      legendOpen,
      genericSelection,
    };
    try {
      localStorage.setItem(canvasPrefsKey(id), JSON.stringify(prefs));
    } catch {
      // Quota/private-mode failures are non-fatal — prefs just don't persist.
    }
  }, [
    project?.id,
    prefsLoadedFor,
    viewMode,
    basemap,
    basemapOpacity,
    nodeVar,
    linkVar,
    schematicAspect,
    nodeScale,
    canvasBackground,
    scaleMode,
    legendOpen,
    genericSelection,
  ]);

  useEffect(() => {
    function onLayoutCommand(e: Event) {
      const mode = (e as CustomEvent<"toggle" | "map" | "schematic">).detail;
      if (mode === "map") {
        setViewMode("map");
      } else if (mode === "schematic") {
        setViewMode("schematic");
      } else {
        setViewMode((v) => (v === "schematic" ? "map" : "schematic"));
      }
    }
    window.addEventListener("hydra:canvas-layout", onLayoutCommand);
    return () =>
      window.removeEventListener("hydra:canvas-layout", onLayoutCommand);
  }, []);

  // ── Map fit key ──────────────────────────────────────────────────────
  // Increments only on project switch so MapCanvas resets its view to fit
  // the new network.  Does NOT increment on scenario switch so the user's
  // chosen pan/zoom position is preserved during scenario comparisons.
  /**
   * What last moved the camera — see `viewportCause`, which explains why
   * this is a cause rather than the boolean it used to be.
   *
   * Starts at `"fit"` because a freshly loaded project is auto-fitted.
   *
   * A ref, not state: nothing renders from it, and making it state would
   * re-render the whole canvas on every pan frame.
   */
  const lastViewportCauseRef = useRef<ViewportCause>("fit");
  const markViewportUserOwned = useCallback(() => {
    lastViewportCauseRef.current = "user";
  }, []);
  const [mapFitKey, setMapFitKey] = useState(0);
  const [zoomInKey, setZoomInKey] = useState(0);
  const [zoomOutKey, setZoomOutKey] = useState(0);
  const [resetNorthKey, setResetNorthKey] = useState(0);

  // ── Measure clicks ──────────────────────────────────────────
  // Map mode only (the tool is disabled in schematic). Each entry is a snapped
  // `[lng, lat]` plus what it snapped to, appended once per click — the canvas
  // keeps no hidden anchor of its own, which is what made the old two-point
  // reconstruction lose the first point and then measure from the wrong origin.
  const [measurePoints, setMeasurePoints] = useState<MeasurePoint[]>([]);

  const clearAnnotations = useCallback(() => {
    setMeasurePoints([]);
  }, []);

  // Positions alone for the canvas overlay; it does not need the snap targets.
  const measurePointPositions = useMemo(
    () => measurePoints.map((p) => p.position),
    [measurePoints],
  );

  /** Append a click, restarting once a pair is complete. */
  const handleMeasurePoint = useCallback(
    (position: [number, number], target: MeasurePoint["target"]) => {
      setMeasurePoints((prev) =>
        prev.length >= 2
          ? [{ position, target }]
          : [...prev, { position, target }],
      );
    },
    [],
  );

  useEffect(() => {
    function onToolCommand(e: Event) {
      const tool = (e as CustomEvent<CanvasTool>).detail;
      // Same gate as the toolbar and keyboard shortcuts: editing tools do
      // not exist for read-only engines, and the palette dispatches through
      // this event too.
      if (
        !modelEditable &&
        (tool === "edit" || tool === "add-node" || tool === "add-link")
      ) {
        return;
      }
      if (tool === "measure") clearAnnotations();
      setActiveTool(tool);
    }
    window.addEventListener("hydra:canvas-tool", onToolCommand);
    return () => window.removeEventListener("hydra:canvas-tool", onToolCommand);
  }, [clearAnnotations, modelEditable]);

  useEffect(() => {
    function onViewportCommand(e: Event) {
      const cmd = (
        e as CustomEvent<
          "zoom-in" | "zoom-out" | "fit" | "reset-north" | "toggle-view"
        >
      ).detail;
      if (cmd === "zoom-in") {
        setZoomInKey((k) => k + 1);
      } else if (cmd === "zoom-out") {
        setZoomOutKey((k) => k + 1);
      } else if (cmd === "fit") {
        setMapFitKey((k) => k + 1);
      } else if (cmd === "reset-north") {
        setResetNorthKey((k) => k + 1);
      } else if (cmd === "toggle-view") {
        handleToggleViewRef.current();
      }
    }
    window.addEventListener("hydra:canvas-viewport", onViewportCommand);
    return () =>
      window.removeEventListener("hydra:canvas-viewport", onViewportCommand);
  }, []);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `project?.id` is an intentional trigger to reset the map viewport on project switch.
  useEffect(() => {
    setMapFitKey((k) => k + 1);
  }, [project?.id]);

  // Stable refs for keyboard handler so it never goes stale on selection changes.
  const selectedNodeIdRef = useRef<string | null>(null);
  const selectedLinkIdRef = useRef<string | null>(null);
  const nodeMapRef = useRef<Map<string, (typeof allNodes)[number]>>(new Map());
  const linkMapRef = useRef<Map<string, (typeof allLinks)[number]>>(new Map());

  // ── Simulation state ─────────────────────────────────────────────
  const {
    resultMeta,
    resultMetaLoading,
    resultGeneration,
    resultsTopologyStale,
  } = useSimulation();
  // `stableResultMeta` lags behind `resultMeta` while metadata is loading.
  // Once loading settles, it mirrors the active scenario exactly (including
  // null for unsimulated scenarios) so overlays cannot bleed across switches.
  const [stableResultMeta, setStableResultMeta] =
    useState<typeof resultMeta>(null);
  // Reset to null when the project changes (different network, stale ranges invalid).
  // biome-ignore lint/correctness/useExhaustiveDependencies: project id is the intentional reset trigger.
  useEffect(() => {
    setStableResultMeta(null);
  }, [project?.id]);
  // Latch while loading, but clear once a scenario settles with no results.
  useEffect(() => {
    if (resultMeta !== null) {
      setStableResultMeta(resultMeta);
      return;
    }
    if (!resultMetaLoading) {
      setStableResultMeta(null);
    }
  }, [resultMeta, resultMetaLoading]);

  // ── Per-period result (fetched on demand when scrubber moves) ─────
  // `currentPeriodResult` holds the flat arrays for exactly one reporting
  // period.  This is the only result data held in component memory — we
  // never load all periods at once.
  const [fetchedPeriodResult, setFetchedPeriodResult] =
    useState<PeriodResults | null>(null);
  // Engine-generic counterpart (catalog-keyed engines): one period of every
  // catalog variable, decoded from the generic payload. Exactly one of the
  // two is ever non-null — the meta's `generic` field decides which decoder
  // the fetch effect uses.
  const [fetchedGenericValues, setFetchedGenericValues] =
    useState<GenericPeriodValues | null>(null);
  /** Which per-period encoding this target serves — the one place that is
   * decided, so the fetch effect and the canvas can never disagree. */
  const periodPath = resultsPath(resultMeta);
  // Which target the held arrays were fetched for. Only needed to tell "this
  // scrub failed, keep what's on screen" apart from "this scenario failed to
  // load at all, so stop showing the last one's colours".
  const loadedTargetRef = useRef<string | null>(null);

  // Topology-stale gate: the loaded results' digest no longer matches the
  // live model (nodes/links added, removed, or renamed), so the flat arrays
  // are index-misaligned with the network. Treat them as absent exactly like
  // a length mismatch — every consumer below (colour overlays, merged sim
  // objects, comparison deltas) reads this gated value. Unknown digests
  // (pre-digest .out files) pass through ungated.
  const currentPeriodResult = resultsTopologyStale ? null : fetchedPeriodResult;

  // ── Engine-generic result channels ─────────────────────────────────
  // For catalog-keyed engines, the selected variable per element class.
  // Empty string = "use the catalog's first variable" (the default until
  // the user picks in the legend).
  const genericMeta = stableResultMeta?.generic ?? null;
  // The catalog drives the legend for every engine; only some engines
  // deliver their period values through it. Everything that reads *values*
  // gates on this, not on the catalog's presence.
  const genericPeriods = resultsPath(stableResultMeta) === "generic";

  const handleGenericSelect = useCallback(
    (cls: GenericClassKey, id: string) => {
      // One store. wds paints from typed variable names whose spellings are
      // its catalog ids, so this selection is all either side needs — and
      // there is no second copy left to fall out of step.
      setGenericSelection((s) => ({ ...s, [cls]: id }));
    },
    [],
  );
  const genericCanvas = useMemo<GenericCanvasResults | null>(() => {
    // Null for engines whose values arrive as fixed arrays, so the canvas
    // takes its own colouring path rather than a catalog channel that will
    // never be filled.
    if (!genericMeta || !genericPeriods) return null;
    const channel = (
      vars: GenericVariable[],
      arrays: Float32Array[] | null,
      selected: string,
    ) => {
      if (vars.length === 0) return null;
      const i = Math.max(
        0,
        vars.findIndex((v) => v.id === selected),
      );
      const values = arrays?.[i] ?? null;
      const v = vars[i];
      // Rescaling to the period rewrites only the range the ramp spans; the
      // variable is otherwise itself, so labels, units and ramp shape are
      // untouched.
      const variable =
        scaleMode === "step"
          ? { ...v, ...periodRange(values, v.min, v.max) }
          : v;
      return { variable, values };
    };
    return {
      node: channel(
        genericMeta.pointVars,
        fetchedGenericValues?.points ?? null,
        genericSelection.point,
      ),
      link: channel(
        genericMeta.polylineVars,
        fetchedGenericValues?.polylines ?? null,
        genericSelection.polyline,
      ),
      region: channel(
        genericMeta.regionVars,
        fetchedGenericValues?.regions ?? null,
        genericSelection.region,
      ),
    };
  }, [
    genericMeta,
    genericPeriods,
    fetchedGenericValues,
    genericSelection,
    scaleMode,
  ]);

  // On project change, discard stale period results immediately: a different
  // project is a different network, so the flat arrays cannot be reinterpreted
  // against it at all.
  //
  // Deliberately NOT on scenario change. Clearing there repainted the whole
  // network in the unsimulated grey scheme for the few frames until the new
  // arrays arrived — reading as "this scenario was never run" — while
  // `stableResultMeta` latched and kept the legend showing simulated. The two
  // are meant to move together; the fetch effect below now latches the period
  // data the same way, and `NetworkDataContext` holds the previous nodes until
  // the new snapshot lands, so the held arrays stay paired with the geometry
  // they were computed for.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `project?.id` is the intentional reset trigger.
  useEffect(() => {
    setFetchedPeriodResult(null);
    setFetchedGenericValues(null);
    loadedTargetRef.current = null;
  }, [project?.id]);

  // Keyed on a value-stable digest of resultMeta rather than its object
  // identity: run completion publishes two fresh (equal) meta objects, which
  // previously triggered a duplicate 1.3 MB period fetch.
  // resultGeneration is a freshness token bumped whenever result metadata is
  // (re)loaded after a run — without it, re-running a simulation whose times
  // are unchanged (value-only edits) would collide in this digest and the
  // period data would never refetch.
  const resultMetaKey = resultMeta
    ? `${resultGeneration}:${resultMeta.times.length}:${resultMeta.times[resultMeta.times.length - 1] ?? 0}:${resultMeta.qualityMode}`
    : null;
  useEffect(() => {
    if (!project?.id) {
      setFetchedPeriodResult(null);
      setFetchedGenericValues(null);
      return;
    }
    if (resultMetaKey == null) {
      // Metadata is null either because this scenario has no simulation, or
      // because it hasn't loaded yet. Only the settled case means "no results":
      // clearing while still loading is what produced the grey flash on every
      // scenario switch. Mirrors the `stableResultMeta` latch above.
      if (!resultMetaLoading) {
        setFetchedPeriodResult(null);
        setFetchedGenericValues(null);
        loadedTargetRef.current = null;
      }
      return;
    }
    if (periodPath === "none") {
      // The engine has result metadata (the timeline steps) but no
      // per-period arrays yet — nothing to fetch, nothing to colour.
      setFetchedPeriodResult(null);
      setFetchedGenericValues(null);
      loadedTargetRef.current = null;
      return;
    }
    const target = `${project.id}:${activeScenarioId ?? "base"}`;
    let cancelled = false;
    // Clamp: on switching to a shorter result set this effect can run before
    // the playhead-clamp effect corrects currentHour, and an out-of-range
    // period would surface a spurious backend error.
    const period = Math.max(
      0,
      Math.min(currentHour, (resultMeta?.times.length ?? 1) - 1),
    );
    if (periodPath === "generic") {
      // Generic-payload engine: same command, generic decoder. The wds
      // arrays stay null so the canvas renders through the generic
      // channels only.
      setFetchedPeriodResult(null);
      getGenericPeriodValues(project.id, period, activeScenarioId)
        .then((r) => {
          if (!cancelled) {
            setFetchedGenericValues(r);
            loadedTargetRef.current = target;
          }
        })
        .catch(() => {
          if (!cancelled && loadedTargetRef.current !== target) {
            setFetchedGenericValues(null);
          }
        });
      return () => {
        cancelled = true;
      };
    }
    setFetchedGenericValues(null);
    getPeriodResults(project.id, period, activeScenarioId)
      .then((r) => {
        if (!cancelled) {
          setFetchedPeriodResult(r);
          loadedTargetRef.current = target;
        }
      })
      // Decode failures reject (already console.error'd in getPeriodResults).
      // Scrubbing within a target can keep the period already on screen. A
      // target we have never loaded cannot: since the switch no longer clears
      // eagerly, keeping it would leave the previous scenario's colours up for
      // as long as the user stayed here.
      .catch(() => {
        if (!cancelled && loadedTargetRef.current !== target) {
          setFetchedPeriodResult(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [
    project?.id,
    currentHour,
    resultMetaKey,
    resultMetaLoading,
    activeScenarioId,
    resultMeta?.times.length,
    // The encoding, not the catalog: this effect chooses a decoder.
    periodPath,
  ]);

  // ── Timeline height CSS variable ─────────────────────────────────
  useEffect(() => {
    const h = projectView === "canvas" ? "64px" : "0px";
    document.documentElement.style.setProperty("--timeline-h", h);
    return () =>
      document.documentElement.style.setProperty("--timeline-h", "0px");
  }, [projectView]);

  // ── Timeline transport ──────────────────────────────────────────
  const [isPlaying, setIsPlaying] = useState(false);
  const [speed, setSpeed] = useState(1); // see PLAYBACK_SPEEDS
  const [loop, setLoop] = useState(true);

  // `maxStep` is the last valid step index: 0..maxStep.
  // Derived from stableResultMeta when available (covers multi-period results),
  // with a fallback for when no simulation has run yet.
  const maxStep = stableResultMeta ? stableResultMeta.times.length - 1 : 24;

  // Quality is only worth showing when the loaded result has quality data;
  // switching to a scenario without it left the picker on an option with
  // nothing behind it and every junction rendering the null-quality grey.
  //
  // All four stores move together — the two the canvas paints from and the
  // two the legend's picker shows. Correcting only the first pair left the
  // legend saying Quality while the hover chip and the inspector showed
  // velocity, and nothing on screen said which to believe.
  const qualityMode = stableResultMeta?.qualityMode ?? "none";
  useEffect(() => {
    setGenericSelection((s) =>
      withQualityAvailability(s, qualityMode !== "none"),
    );
  }, [qualityMode]);
  // Derived from the *loaded result*, not current simParams: editing the
  // duration without re-running must not flip the banner/scrubber for a
  // result that was produced under the old settings.
  const isSteadyState = stableResultMeta
    ? stableResultMeta.times.length <= 1
    : simParams != null && simParams.duration <= 0;

  // Clamp the playhead when switching between result sets with different lengths
  // (e.g. transient -> steady-state) so period fetches stay in range.
  useEffect(() => {
    setCurrentHour((h) => Math.max(0, Math.min(maxStep, h)));
  }, [maxStep]);

  // Auto-advance the playhead. See `stepIntervalMs` for what 1× means.
  useEffect(() => {
    if (!isPlaying) return;
    const intervalMs = stepIntervalMs(speed);
    const id = window.setInterval(() => {
      setCurrentHour((h) => {
        if (h >= maxStep) {
          if (loop) return 0;
          setIsPlaying(false);
          return h;
        }
        return h + 1;
      });
    }, intervalMs);
    return () => window.clearInterval(id);
  }, [isPlaying, speed, loop, maxStep]);

  // Keyboard transport: Space = play/pause, ←/→ = step, Home/End = jump.
  // Tool shortcuts: S/E/N/L/D switch tools; Escape returns to Select.
  // Guard: only handle keys when canvas tab is active (all tabs are always mounted).
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (projectView !== "canvas") return;
      const target = e.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.isContentEditable)
      )
        return;
      // Never hijack OS/app shortcuts: with Cmd/Ctrl/Alt held these keys are
      // chords (Cmd+S save, Cmd+L, Alt-composed characters…), not tool
      // hotkeys or transport controls.
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      switch (e.key) {
        case " ":
          e.preventDefault();
          setIsPlaying((v) => !v);
          break;
        case "ArrowLeft":
          e.preventDefault();
          setCurrentHour((h) => Math.max(0, h - 1));
          break;
        case "ArrowRight":
          e.preventDefault();
          setCurrentHour((h) => Math.min(maxStep, h + 1));
          break;
        case "Home":
          e.preventDefault();
          setCurrentHour(0);
          break;
        case "End":
          e.preventDefault();
          setCurrentHour(maxStep);
          break;
        case "s":
        case "S":
          setActiveTool("select");
          break;
        case "d":
        case "D":
          // Map mode only, matching the toolbar button's `disabled` state.
          // Without this the shortcut was the one way into measure mode in
          // schematic view, where it has no meaningful coordinate space to
          // measure in.
          if (geographic) {
            setActiveTool("measure");
            clearAnnotations();
          }
          break;
        // Map-only tools are gated here as well as on their buttons: the
        // shortcut was the one way to reach them in schematic view, where a
        // placed or moved node would take a coordinate from the synthetic BFS
        // layout rather than the network's own geometry.
        case "e":
        case "E":
          if (geographic && modelEditable) setActiveTool("edit");
          break;
        case "n":
        case "N":
          if (geographic && modelEditable) setActiveTool("add-node");
          break;
        // Not map-gated: creating a link writes only its two node ids.
        case "l":
        case "L":
          if (modelEditable) setActiveTool("add-link");
          break;
        case "Escape":
          // The criteria panel is the topmost thing Escape can mean, and
          // one press must not both close it and reset the tool behind it.
          // Handled here rather than by a second window listener, so the
          // precedence is written down instead of decided by bind order.
          if (criteriaOpenRef.current) {
            setCriteriaOpen(false);
            break;
          }
          setActiveTool("select");
          break;
        case "Delete":
        case "Backspace": {
          const nid = selectedNodeIdRef.current;
          const lid = selectedLinkIdRef.current;
          if (nid) {
            const node = nodeMapRef.current.get(nid);
            if (node) setPendingDelete({ kind: node.type, id: nid });
          } else if (lid) {
            const link = linkMapRef.current.get(lid);
            if (link) setPendingDelete({ kind: link.type, id: lid });
          }
          break;
        }
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [clearAnnotations, maxStep, projectView, geographic, modelEditable]);

  const baseNodes = useNodes();
  const baseLinks = useLinks();
  const baseRegions = useRegions();
  // Hydraulic connections that are not links (dual-drainage street
  // inlets): the layout counts them as connectivity, the canvas draws
  // them. Empty for engines without them.
  const { couplings: inletCouplings, resolved: couplingsResolved } =
    useInletCouplings(project?.id, activeScenarioId);
  // The canvas owns the timeline, but sibling views (the element tables)
  // ask the same question, so the value is published rather than lifted —
  // the scrub state keeps its playback and clamping logic here.
  const publishPeriod = usePublishCurrentPeriod();
  useEffect(() => {
    publishPeriod(currentHour);
  }, [currentHour, publishPeriod]);

  // Raw committed snapshot (source-CRS coords) for undo capture inside
  // stable callbacks — same render-time-ref pattern as the selection refs
  // below. Never use nodeMap/allNodes for capture: those carry *reprojected*
  // coordinates and merged sim values.
  const rawNetworkRef = useRef({ nodes: baseNodes, links: baseLinks });
  rawNetworkRef.current = { nodes: baseNodes, links: baseLinks };

  // Dismissible notice explaining why result overlays vanished after a
  // structural edit (topology digest mismatch); re-arms when staleness
  // clears (fresh run, or the edit is undone) so a later drift re-notifies.
  const [staleNoticeDismissed, setStaleNoticeDismissed] = useState(false);
  useEffect(() => {
    if (!resultsTopologyStale) setStaleNoticeDismissed(false);
  }, [resultsTopologyStale]);
  // ── CRS reprojection ────────────────────────────────────────────────────
  // See useCrsReprojection: source-CRS mirror of the project row, lazy proj4
  // def resolution, WGS84 reprojection of node coords + link vertices (with
  // per-element identity caches), and coordinate-coverage classification.
  const {
    sourceCrs,
    crsError,
    crsResolving,
    coordStatus,
    coordMissingCount,
    rawPositionNodes,
    posNodes,
    canvasLinks,
    canvasRegions,
  } = useCrsReprojection({
    projectSourceCrs: project?.sourceCrs,
    baseNodes,
    baseLinks,
    baseRegions,
  });

  // O(1) enrichment flag (replaces the previous 46k `.some` scans): true when
  // the current period result matches the network's node order/length.
  // `currentPeriodResult` is already null when the topology digest says the
  // results are stale (see the gate above), so this guard covers both the
  // length mismatch and the digest mismatch.
  const simMerged =
    currentPeriodResult != null &&
    currentPeriodResult.nodePressure.length === baseNodes.length;

  // Merged per-element objects are consumed only by the rail list, command
  // palette, and inspector. When none of those is visible, skip the ~92k
  // object spreads per timeline step entirely — the canvas doesn't need them.
  const needSimObjects =
    railOpen ||
    commandPaletteOpen ||
    selectedNodeId != null ||
    selectedLinkId != null;

  const allNodes = useMemo(() => {
    if (!needSimObjects || !simMerged || !currentPeriodResult) return posNodes;
    return posNodes.map((n, i) => ({
      ...n,
      ...nodeResultsAt(currentPeriodResult, i),
    }));
  }, [posNodes, currentPeriodResult, simMerged, needSimObjects]);

  const allLinks = useMemo(() => {
    if (
      !needSimObjects ||
      !currentPeriodResult ||
      currentPeriodResult.linkFlow.length !== baseLinks.length
    ) {
      return baseLinks;
    }
    return baseLinks.map((l, i) => ({
      ...l,
      ...linkResultsAt(currentPeriodResult, i),
    }));
  }, [baseLinks, currentPeriodResult, needSimObjects]);

  // Current-period catalog values for the selected element — rendered by
  // the per-engine inspector bodies (registry slot prop). Payload arrays
  // share the snapshot order of baseNodes/baseLinks, so the element's
  // array index is its position there.
  const genericNodeResults = useMemo(() => {
    if (!genericMeta || !fetchedGenericValues || selectedNodeId == null) {
      return null;
    }
    const si = baseNodes.findIndex((n) => n.id === selectedNodeId);
    if (si < 0) return null;
    return genericMeta.pointVars.map((v, i) => ({
      id: v.id,
      label: v.label,
      quantity: v.quantity,
      value: fetchedGenericValues.points[i]?.[si] ?? null,
      primary: v.id === genericCanvas?.node?.variable.id,
    }));
  }, [
    genericMeta,
    fetchedGenericValues,
    selectedNodeId,
    baseNodes,
    genericCanvas?.node,
  ]);

  const genericLinkResults = useMemo(() => {
    if (!genericMeta || !fetchedGenericValues || selectedLinkId == null) {
      return null;
    }
    const si = baseLinks.findIndex((l) => l.id === selectedLinkId);
    if (si < 0) return null;
    return genericMeta.polylineVars.map((v, i) => ({
      id: v.id,
      label: v.label,
      quantity: v.quantity,
      value: fetchedGenericValues.polylines[i]?.[si] ?? null,
      primary: v.id === genericCanvas?.link?.variable.id,
    }));
  }, [
    genericMeta,
    fetchedGenericValues,
    selectedLinkId,
    baseLinks,
    genericCanvas?.link,
  ]);

  // The selected region object, resolved from the canvas-projected array
  // the map renders (its ring is what "zoom to feature" fits).
  const selectedRegion = useMemo(
    () =>
      selectedRegionId == null
        ? null
        : (canvasRegions.find((r) => r.id === selectedRegionId) ?? null),
    [canvasRegions, selectedRegionId],
  );

  const genericRegionResults = useMemo(() => {
    if (!genericMeta || !fetchedGenericValues || selectedRegionId == null) {
      return null;
    }
    const si = baseRegions.findIndex((r) => r.id === selectedRegionId);
    if (si < 0) return null;
    return genericMeta.regionVars.map((v, i) => ({
      id: v.id,
      label: v.label,
      quantity: v.quantity,
      value: fetchedGenericValues.regions[i]?.[si] ?? null,
      primary: v.id === genericCanvas?.region?.variable.id,
    }));
  }, [
    genericMeta,
    fetchedGenericValues,
    selectedRegionId,
    baseRegions,
    genericCanvas?.region,
  ]);

  // Keep the selection context's sim data in sync so the rail can display
  // live result values without re-fetching from the backend. Always push:
  // when the period result matches the network the arrays are merged with sim
  // values; after a topology change they are the fresh raw arrays — holding
  // back in that case left deleted elements listed in the rail forever, since
  // no period refetch arrives until the next run.
  // For generic-results engines, merge current-period values onto each
  // element (`resultValues`, keyed by variable id) so the rail list gets
  // live result columns — the generic counterpart of the wds pressure/flow
  // merge above. The column set is the catalog's leading variables, capped
  // to keep the rail readable; order guarantee as above (payload arrays
  // share allNodes/baseLinks order).
  const railNodeColumns = useMemo(
    () => railColumns(genericMeta?.pointVars ?? [], genericSelection.point),
    [genericMeta, genericSelection.point],
  );
  const railLinkColumns = useMemo(
    () =>
      railColumns(genericMeta?.polylineVars ?? [], genericSelection.polyline),
    [genericMeta, genericSelection.polyline],
  );
  const railNodes = useMemo(() => {
    const arrays = fetchedGenericValues?.points;
    if (
      !needSimObjects ||
      !arrays ||
      railNodeColumns.length === 0 ||
      arrays[0]?.length !== allNodes.length
    ) {
      return allNodes;
    }
    return allNodes.map((n, i) => {
      const resultValues: Record<string, number | null> = {};
      railNodeColumns.forEach((c) => {
        const v = arrays[c.at]?.[i];
        resultValues[c.key] = v != null && Number.isFinite(v) ? v : null;
      });
      return { ...n, resultValues };
    });
  }, [allNodes, fetchedGenericValues?.points, railNodeColumns, needSimObjects]);
  const railLinks = useMemo(() => {
    const arrays = fetchedGenericValues?.polylines;
    if (
      !needSimObjects ||
      !arrays ||
      railLinkColumns.length === 0 ||
      arrays[0]?.length !== allLinks.length
    ) {
      return allLinks;
    }
    return allLinks.map((l, i) => {
      const resultValues: Record<string, number | null> = {};
      railLinkColumns.forEach((c) => {
        const v = arrays[c.at]?.[i];
        resultValues[c.key] = v != null && Number.isFinite(v) ? v : null;
      });
      return { ...l, resultValues };
    });
  }, [
    allLinks,
    fetchedGenericValues?.polylines,
    railLinkColumns,
    needSimObjects,
  ]);

  const railRegionColumns = useMemo(
    () => railColumns(genericMeta?.regionVars ?? [], genericSelection.region),
    [genericMeta, genericSelection.region],
  );
  const railRegions = useMemo(() => {
    const arrays = fetchedGenericValues?.regions;
    if (
      !needSimObjects ||
      !arrays ||
      railRegionColumns.length === 0 ||
      arrays[0]?.length !== baseRegions.length
    ) {
      return baseRegions;
    }
    return baseRegions.map((r, i) => {
      const resultValues: Record<string, number | null> = {};
      railRegionColumns.forEach((c) => {
        const v = arrays[c.at]?.[i];
        resultValues[c.key] = v != null && Number.isFinite(v) ? v : null;
      });
      return { ...r, resultValues };
    });
  }, [
    baseRegions,
    fetchedGenericValues?.regions,
    railRegionColumns,
    needSimObjects,
  ]);

  // An engine with fixed variables gets the same treatment: its legend
  // choice becomes the rail's column, reading a field the sim merge already
  // put on each element rather than a catalog bag.
  const wdsColumns = useMemo(
    () => ({
      node: [
        {
          key: nodeVar,
          label: WDS_NODE_VARS[nodeVar].label,
          symbol: WDS_NODE_VARS[nodeVar].symbol,
          unit: WDS_NODE_VARS[nodeVar].unit,
        },
      ],
      link: [
        {
          key: linkVar,
          label: WDS_LINK_VARS[linkVar].label,
          symbol: WDS_LINK_VARS[linkVar].symbol,
          unit: WDS_LINK_VARS[linkVar].unit,
        },
      ],
      region: [],
    }),
    [nodeVar, linkVar],
  );

  useEffect(() => {
    setSimData(
      railNodes,
      railLinks,
      railRegions,
      genericMeta
        ? {
            node: railNodeColumns,
            link: railLinkColumns,
            region: railRegionColumns,
          }
        : wdsColumns,
    );
  }, [
    railNodes,
    railLinks,
    railRegions,
    railNodeColumns,
    wdsColumns,
    railLinkColumns,
    railRegionColumns,
    genericMeta,
    setSimData,
  ]);

  // Locate the network-wide min/max of the active variable for the current
  // period, then select and fly to that element. Period arrays are index-
  // aligned with posNodes / baseLinks.
  const onLocateExtreme = useCallback(
    (target: "node" | "link", which: "min" | "max") => {
      const pr = currentPeriodResult;
      if (!pr) return;
      const pick = (
        ids: { id: string }[],
        arr: ArrayLike<number> | null | undefined,
      ): string | null => {
        if (!arr) return null;
        let bestId: string | null = null;
        let bestVal = which === "min" ? Infinity : -Infinity;
        for (let i = 0; i < ids.length; i += 1) {
          const v = arr[i];
          if (v == null || !Number.isFinite(v)) continue;
          if (which === "min" ? v < bestVal : v > bestVal) {
            bestVal = v;
            bestId = ids[i].id;
          }
        }
        return bestId;
      };
      if (target === "node") {
        const arr =
          nodeVar === "pressure"
            ? pr.nodePressure
            : nodeVar === "head"
              ? pr.nodeHead
              : nodeVar === "demand"
                ? pr.nodeDemand
                : pr.nodeQuality;
        const id = pick(posNodes, arr);
        if (id) {
          selectNode(id);
          setFlyToState((s) => ({
            nodeId: id,
            linkId: null,
            regionId: null,
            key: s.key + 1,
          }));
        }
      } else {
        const arr =
          linkVar === "flow"
            ? pr.linkFlow
            : linkVar === "velocity"
              ? pr.linkVelocity
              : linkVar === "headloss"
                ? pr.linkHeadloss
                : linkVar === "quality"
                  ? pr.linkQuality
                  : null;
        const id = pick(baseLinks, arr);
        if (id) {
          selectLink(id);
          setFlyToState((s) => ({
            nodeId: null,
            linkId: id,
            regionId: null,
            key: s.key + 1,
          }));
        }
      }
    },
    [
      currentPeriodResult,
      nodeVar,
      linkVar,
      posNodes,
      baseLinks,
      selectNode,
      selectLink,
    ],
  );

  /**
   * The ranges the wds canvas and legend scale against.
   *
   * The catalog-driven path rescales itself per period inside
   * `genericCanvas`; wds colours from these fixed props instead, so
   * without this the scale control was offered and did nothing — the
   * ramp and its numbers were the run's range at every step.
   *
   * `periodRange` supplies the guard: a period whose span is a sliver of
   * the run's keeps the run's range, so a near-uniform field is not
   * amplified into a picture of its own rounding.
   */
  const wdsPeriodRange = useCallback(
    (
      values: Float32Array | null | undefined,
      runMin: number,
      runMax: number,
    ) =>
      scaleMode === "step" && currentPeriodResult
        ? periodRange(values ?? null, runMin, runMax)
        : { min: runMin, max: runMax },
    [scaleMode, currentPeriodResult],
  );

  const wdsRanges = useMemo(() => {
    const r = stableResultMeta?.ranges;
    const pr = currentPeriodResult;
    const head = wdsPeriodRange(
      pr?.nodeHead,
      r?.headMin ?? 0,
      r?.headMax ?? 100,
    );
    const demand = wdsPeriodRange(
      pr?.nodeDemand,
      r?.demandMin ?? 0,
      r?.demandMax ?? 1,
    );
    const pressure = wdsPeriodRange(
      pr?.nodePressure,
      r?.pressureMin ?? 0,
      r?.pressureMax ?? 100,
    );
    const flow = wdsPeriodRange(pr?.linkFlow, r?.flowMin ?? 0, r?.flowMax ?? 1);
    const velocity = wdsPeriodRange(
      pr?.linkVelocity,
      r?.velocityMin ?? 0,
      r?.velocityMax ?? 1.5,
    );
    const quality = wdsPeriodRange(
      pr?.nodeQuality,
      r?.qualityMin ?? 0,
      r?.qualityMax ?? 1,
    );
    return { head, demand, pressure, flow, velocity, quality };
  }, [stableResultMeta, currentPeriodResult, wdsPeriodRange]);

  /** The selected variable's range, for the legend's numbers. */
  const wdsRangeFor = useCallback(
    (id: string) => {
      const byId: Record<string, { min: number; max: number } | undefined> = {
        pressure: wdsRanges.pressure,
        head: wdsRanges.head,
        demand: wdsRanges.demand,
        flow: wdsRanges.flow,
        velocity: wdsRanges.velocity,
        quality: wdsRanges.quality,
      };
      return byId[id];
    },
    [wdsRanges],
  );

  // ── Per-engine legend affordances ─────────────────────────────────────────
  // Supplied to the shared legend rather than branched on inside it. An
  // engine with no criteria bands, no locatable extremes, or no animatable
  // quantity simply contributes nothing and the control is absent.

  /** The legend speaks in element classes; the wds extremes search is
   * indexed by node/link arrays. Regions have no wds counterpart. */
  const handleLocateExtreme = useCallback(
    (cls: GenericClassKey, which: "min" | "max") => {
      if (cls === "region") return;
      onLocateExtreme(cls === "point" ? "node" : "link", which);
    },
    [onLocateExtreme],
  );

  // ── Clear view ────────────────────────────────────────────────────────────
  //
  // Dismisses everything stacked over the map, and nothing else. Saved
  // preferences — the chosen variables, the scale, the basemap, the dim
  // toggle — are what the map *means* to this reader and survive: the
  // button tidies the view, it does not undo decisions.
  //
  // The camera is deliberately untouched. "Fit network" sits directly
  // above and is that action; a clear that also moved the viewport would
  // be the most disorienting control on the canvas.
  const clearable = useMemo<ClearableView>(
    () => ({
      rail: railOpen,
      selection:
        selectedNodeId != null ||
        selectedLinkId != null ||
        selectedRegionId != null,
      legend: legendOpen,
      basemapMenu: showBasemapDropdown,
      tool: activeTool !== "select",
      measurements: measurePoints.length > 0,
    }),
    [
      railOpen,
      selectedNodeId,
      selectedLinkId,
      selectedRegionId,
      legendOpen,
      showBasemapDropdown,
      activeTool,
      measurePoints.length,
    ],
  );
  const viewAction = useMemo(() => viewButtonAction(clearable), [clearable]);

  // The viewport-command listener mounts once, so it reads the handler
  // through a ref rather than closing over the render that installed it —
  // otherwise the palette would clear a view as it stood at mount.
  const handleToggleViewRef = useRef<() => void>(() => {});

  const handleToggleView = useCallback(() => {
    if (viewButtonAction(clearable) === "restore") {
      // Nothing is covering the map, so the press means the opposite: give
      // the panels back. Only the two that can be given back — see
      // `viewButtonAction`.
      if (!clearable.rail) toggleRail();
      setLegendOpen(true);
      // Opening the rail shrinks the map exactly as closing it grew it, so
      // a framing the app owns stops being a fit either way.
      if (
        shouldRefitAfterOcclusionChange(
          !viewportIsUserOwned(lastViewportCauseRef.current),
          true,
        )
      ) {
        setMapFitKey((k) => k + 1);
      }
      return;
    }
    // Only these actually shrink the map; the rest sit over it without
    // changing what Fit has to work with.
    const occlusionChanged = clearable.rail || clearable.selection;
    // `toggleRail` is a toggle, so guard on the current state rather than
    // calling it unconditionally — an unguarded call would *open* the rail
    // for anyone whose view was already clear.
    if (clearable.rail) toggleRail();
    if (clearable.selection) clearSelection();
    if (clearable.legend) setLegendOpen(false);
    if (clearable.basemapMenu) setShowBasemapDropdown(false);
    if (clearable.tool) setActiveTool("select");
    if (clearable.measurements) clearAnnotations();
    if (
      shouldRefitAfterOcclusionChange(
        !viewportIsUserOwned(lastViewportCauseRef.current),
        occlusionChanged,
      )
    ) {
      setMapFitKey((k) => k + 1);
    }
  }, [clearable, toggleRail, clearSelection, clearAnnotations]);

  handleToggleViewRef.current = handleToggleView;

  const legendAnimation = useMemo(
    () => ({
      playing: linkAnimation,
      // From the registry, not from this engine's list: the legend below
      // is every engine's, and the sentence it builds for a reader whose
      // selection is not animated is built from these ids.
      appliesTo: animatedVariables,
      onToggle: setLinkAnimation,
      reducedMotion,
    }),
    [linkAnimation, setLinkAnimation, reducedMotion, animatedVariables],
  );

  /** Read-only band text under a criteria-backed variable's ramp. The
   * legend shows where the bands fall on the scale; Analysis is where they
   * are edited. */
  const criteriaAnnotation = useCallback(
    (variableId: string): string | null => {
      const show = (v: number, q: Quantity) =>
        `${Number(toDisplay(v, q, unitSystem).toFixed(2))}`;
      if (variableId === "pressure") {
        const b = criteria.pressure;
        return `< ${show(b.low, "pressure")} low · ${show(b.required, "pressure")} required · > ${show(b.high, "pressure")} high`;
      }
      if (variableId === "velocity" || variableId === "flow") {
        const b = variableId === "velocity" ? criteria.velocity : criteria.flow;
        const q: Quantity = variableId === "velocity" ? "velocity" : "flow";
        return `< ${show(b.low, q)} low · ${show(b.target, q)} target · > ${show(b.high, q)} high`;
      }
      return null;
    },
    [criteria, unitSystem],
  );

  // MapCanvas gets the *stable* position/base arrays plus the flat period
  // result — colours update via the periodResult prop without new arrays, so
  // the old flicker-latch over merged arrays is no longer needed. During the
  // brief window after a non-topology edit the previous period result still
  // matches by length and keeps the canvas coloured; after a topology change
  // the length guard in MapCanvas drops stale colours immediately.
  const canvasNodes = posNodes;

  // Coordinates that could not be reprojected are not positions. Handing
  // them to the map draws the network at whatever longitude and latitude
  // those raw numbers happen to name — a fabricated place, under an overlay
  // saying the placement is invalid, which reads as a warning about a map
  // that is fine. Nothing is the honest picture, and it is also what makes
  // the overlay legible.
  //
  // Not gated on `crsResolving` the way the overlay is: a blank moment
  // while a definition loads is better than a moment of confident nonsense.
  const placeable = viewMode !== "map" || !crsError;
  const shownNodes = placeable ? canvasNodes : EMPTY_NODES;
  const shownLinks = placeable ? canvasLinks : EMPTY_LINKS;
  const shownRegions = placeable ? canvasRegions : EMPTY_REGIONS;

  const nodeMap = useMemo(
    () => new Map(allNodes.map((n) => [n.id, n])),
    [allNodes],
  );

  const linkMap = useMemo(
    () => new Map(allLinks.map((l) => [l.id, l])),
    [allLinks],
  );

  const selectedNode = selectedNodeId
    ? (nodeMap.get(selectedNodeId) ?? null)
    : null;
  const selectedLink = selectedLinkId
    ? (linkMap.get(selectedLinkId) ?? null)
    : null;

  // Keep the last *enriched* node/link object so the inspector card shows
  // stale sim values instead of blanking out ("—") during the brief window
  // when bumpNetwork() has delivered new baseNodes (pressure: null) but
  // getPeriodResults() hasn't resolved yet for the new scenario.
  // Rules:
  //   • If no simulation is loaded (!stableResultMeta): always use the latest
  //     raw node (static props only, card shows EmptyStateCard).
  //   • If a simulation is loaded: only update the ref when the node is
  //     actually enriched with hydraulic results. Hold the last enriched
  //     version until new data arrives so the card never flashes dashes.
  //   • When the user deselects (id → null): clear immediately.
  const stableSelectedNodeRef = useRef<typeof selectedNode>(null);
  const stableSelectedLinkRef = useRef<typeof selectedLink>(null);
  const nodeIsEnriched =
    selectedNode !== null &&
    (!stableResultMeta ||
      selectedNode.pressure != null ||
      selectedNode.demand != null ||
      selectedNode.head != null ||
      selectedNode.quality != null);
  const linkIsEnriched =
    selectedLink !== null &&
    (!stableResultMeta ||
      selectedLink.flow != null ||
      selectedLink.status != null ||
      selectedLink.quality != null);
  if (nodeIsEnriched) stableSelectedNodeRef.current = selectedNode;
  if (linkIsEnriched) stableSelectedLinkRef.current = selectedLink;
  if (selectedNodeId === null) stableSelectedNodeRef.current = null;
  if (selectedLinkId === null) stableSelectedLinkRef.current = null;
  // During a transition (node exists but isn't enriched yet), prefer the cached
  // enriched object so the card keeps showing the old values rather than "—".
  // Fall back to the live node only when no cached version exists (first select).
  const stableSelectedNode = selectedNodeId
    ? nodeIsEnriched
      ? selectedNode
      : (stableSelectedNodeRef.current ?? selectedNode)
    : null;
  const stableSelectedLink = selectedLinkId
    ? linkIsEnriched
      ? selectedLink
      : (stableSelectedLinkRef.current ?? selectedLink)
    : null;

  const selectedNodeHasCoordinates =
    stableSelectedNode != null &&
    !(stableSelectedNode.x === 0 && stableSelectedNode.y === 0);
  const selectedLinkHasCoordinates =
    stableSelectedLink != null &&
    (() => {
      const from = nodeMap.get(stableSelectedLink.fromId);
      const to = nodeMap.get(stableSelectedLink.toId);
      if (!from || !to) return false;
      return !(from.x === 0 && from.y === 0) && !(to.x === 0 && to.y === 0);
    })();

  // Keep stable refs in sync so the keyboard handler can read the current
  // selection without being re-registered on every selection change.
  selectedNodeIdRef.current = selectedNodeId;
  selectedLinkIdRef.current = selectedLinkId;
  nodeMapRef.current = nodeMap;
  linkMapRef.current = linkMap;

  // Mutual-deselection handlers: delegate to context which handles toggle logic.
  // The inspector only opens when the Select tool is active; other tools just
  // update the selected-id so halo/highlight state works without popping the panel.
  const handleSelectNode = useCallback(
    (id: string | null) => {
      if (activeTool !== "select") {
        setSelectedNodeId(id);
      } else {
        selectNode(id);
      }
    },
    [activeTool, selectNode, setSelectedNodeId],
  );
  const handleSelectLink = useCallback(
    (id: string | null) => {
      if (activeTool !== "select") {
        setSelectedLinkId(id);
      } else {
        selectLink(id);
      }
    },
    [activeTool, selectLink, setSelectedLinkId],
  );

  // Close the inspector whenever the user switches away from the Select tool.
  useEffect(() => {
    if (activeTool !== "select") setInspectorView("closed");
  }, [activeTool, setInspectorView]);

  // Publish the inspector's occupied width so canvas overlays pinned to the
  // right edge can stay clear of it — the mirror of `--rail-effective-w` on the
  // left. The condition must match the panel's own render condition below, or
  // overlays shift for a panel that never appears.
  const inspectorOccupies =
    (inspectorView === "node" && stableSelectedNode != null) ||
    (inspectorView === "link" && stableSelectedLink != null) ||
    (inspectorView === "region" && selectedRegion != null);
  useEffect(() => {
    // Resolved to a length rather than left as `var(--inspector-w)`: CSS
    // calc() would cope either way, but the canvas reads this from script to
    // work out how much of itself is covered, and script gets the raw string.
    const width = getComputedStyle(document.documentElement)
      .getPropertyValue("--inspector-w")
      .trim();
    document.documentElement.style.setProperty(
      "--inspector-effective-w",
      inspectorOccupies && width ? width : "0px",
    );
    return () => {
      document.documentElement.style.setProperty(
        "--inspector-effective-w",
        "0px",
      );
    };
  }, [inspectorOccupies]);

  // Reset to Select when switching to Schematic if the active tool is map-only.
  //
  // What makes a tool map-only is that it reads or writes a *coordinate*:
  // `add-node` and `edit` place one, and the schematic's positions are synthetic
  // BFS output rather than the network's own geometry; `measure` reports a
  // distance, which is meaningless in that space. `add-link` writes only its two
  // node ids, so it works anywhere — and schematic is arguably where connecting
  // nodes is easiest to see.
  useEffect(() => {
    if (
      viewMode === "schematic" &&
      (activeTool === "edit" ||
        activeTool === "add-node" ||
        activeTool === "measure")
    ) {
      setActiveTool("select");
    }
  }, [viewMode, activeTool]);

  const canvasIsActive = isActive && projectView === "canvas";

  // The inspector's entrance animation should play when the panel appears,
  // not when its contents change kind. Node, link and region bodies are
  // separate components, so following a "connected to" link from a node to
  // a conduit unmounts one and mounts the other — replaying a fade-in over
  // the canvas for what the reader experiences as the same panel showing
  // something else. Clicking around the map never did this, because it
  // usually keeps you within one kind.
  const prevInspectorViewRef = useRef<InspectorView>("closed");
  const inspectorEntering = prevInspectorViewRef.current === "closed";
  useEffect(() => {
    prevInspectorViewRef.current = inspectorView;
  }, [inspectorView]);

  // Shared styling for toolbar controls that only work on the geographic map.
  const mapOnly = !geographic;

  const handleNodeMoved = useCallback(
    async (
      id: string,
      lng: number,
      lat: number,
    ): Promise<undefined | false> => {
      if (!project) return;
      // MapLibre hands us the drop point in WGS84 — the backend coordinate
      // store holds SOURCE-CRS values, so inverse-project before patching.
      // (Identity when sourceCrs is EPSG:4326.) Committing raw lng/lat into
      // a projected store corrupts the coordinate.
      let x: number;
      let y: number;
      try {
        [x, y] = wgs84ToSourceCrs([lng, lat], sourceCrs);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        showToast(`Move cancelled — ${msg}`, "error");
        // `false` tells MapCanvas the move was not committed: it clears the
        // drag preview so the node snaps back to its stored position.
        return false;
      }
      // Undo capture: previous raw (source-CRS) coordinates, read BEFORE the
      // patch — same coordinate space as the converted values patched below.
      const prev = rawNetworkRef.current.nodes.find((n) => n.id === id);
      await patchNodePosition(id, x, y);
      if (prev) {
        // Position inverses travel as x/y field patches (same coordinate
        // store as patch_node_position) — see undoStack's RecreateSpec doc.
        pushUndoEntry(stackKey(project.id, activeScenarioId ?? null), {
          label: `Moved ${id}`,
          undo: {
            patches: [
              { kind: prev.type, id, field: "x", value: prev.x },
              { kind: prev.type, id, field: "y", value: prev.y },
            ],
          },
          redo: {
            patches: [
              { kind: prev.type, id, field: "x", value: x },
              { kind: prev.type, id, field: "y", value: y },
            ],
          },
        });
      }
      await saveProjectOnDisk(project.id, activeScenarioId);
      markEdited(project.id, activeScenarioId);
      // No bumpNetwork(): the backend emits `network-changed`, which already
      // bumps the version — a manual bump doubled the full-snapshot refetch.
    },
    [project, activeScenarioId, markEdited, sourceCrs, showToast],
  );

  const handleConfirmDelete = useCallback(async () => {
    if (!pendingDelete || !project) return;
    const { kind, id } = pendingDelete;
    setPendingDelete(null);
    clearSelection();
    // Undo capture: the element plus any links that cascade-delete with a
    // node, from the raw snapshot BEFORE the delete.
    const { nodes: rawNodes, links: rawLinks } = rawNetworkRef.current;
    const recreates = recreateSpecsForDelete(kind, id, rawNodes, rawLinks);
    try {
      await deleteElement(kind, id);
    } catch (err) {
      // A refused delete must surface, not vanish as an unhandled
      // rejection with the element silently still present.
      showToast(`Could not delete ${id}: ${err}`, "error");
      return;
    }
    if (recreates) {
      pushUndoEntry(stackKey(project.id, activeScenarioId ?? null), {
        label: `Deleted ${id}`,
        undo: { recreates },
        redo: { deletes: [{ kind, id }] },
      });
    }
    await saveProjectOnDisk(project.id, activeScenarioId);
    markEdited(project.id, activeScenarioId);
    // No bumpNetwork(): backend event already bumps (see handleNodeMoved).
  }, [
    pendingDelete,
    project,
    activeScenarioId,
    markEdited,
    clearSelection,
    showToast,
  ]);

  const handleRenameElement = useCallback(
    async (kind: string, oldId: string, rawNewId: string) => {
      const ok = await renameElementFlow(kind, oldId, rawNewId);
      if (!ok) return;
      // Keep the renamed element selected under its new id (the backend
      // `network-changed` event drives the refetch that repopulates it).
      // Node-vs-link is decided by which array holds the element — a kind
      // allowlist misrouted every non-wds link kind to the node selector.
      const newId = rawNewId.trim();
      if (linkMapRef.current.has(oldId)) selectLink(newId);
      else selectNode(newId);
    },
    [renameElementFlow, selectNode, selectLink],
  );

  // ── Node / link ID suggestion ─────────────────────────────────────────────
  // Generates a short unique ID by finding the first gap in the existing IDs.
  // Accepts the node kind ("junction" | "reservoir" | "tank") and picks the
  // appropriate prefix automatically.
  const suggestNodeId = useCallback(
    (kind: string) => {
      const prefix = NODE_KIND_PREFIX[kind] ?? "N";
      const existing = new Set(allNodes.map((n) => n.id));
      for (let i = 1; i <= 9999; i++) {
        const id = `${prefix}${i}`;
        if (!existing.has(id)) return id;
      }
      return `${prefix}${Date.now()}`;
    },
    [allNodes],
  );

  const suggestLinkId = useCallback(
    (kind: string) => {
      const prefix = kind === "pump" ? "PU" : kind === "valve" ? "V" : "P";
      const existing = new Set(allLinks.map((l) => l.id));
      for (let i = 1; i <= 9999; i++) {
        const id = `${prefix}${i}`;
        if (!existing.has(id)) return id;
      }
      return `${prefix}${Date.now()}`;
    },
    [allLinks],
  );

  const handleCreateNodeRequest = useCallback((lng: number, lat: number) => {
    setPendingCreateNode({ lng, lat });
  }, []);

  const handleCreateLinkRequest = useCallback(
    (fromId: string, toId: string) => {
      setPendingCreateLink({ fromId, toId });
    },
    [],
  );

  const handleConfirmCreateNode = useCallback(
    async (payload: NodeCreatePayload) => {
      if (!pendingCreateNode || !project) return;
      const { lng, lat } = pendingCreateNode;
      // The map click arrives in WGS84 but the backend coordinate store holds
      // SOURCE-CRS values — inverse-project before creating (identity for
      // EPSG:4326). Throws on failure, which the modal catches and displays,
      // so an unconvertible point never commits a corrupt coordinate.
      const [x, y] = wgs84ToSourceCrs([lng, lat], sourceCrs);
      // Throws on backend error — the modal catches and stays open with the error message.
      await createNode(
        payload.kind,
        payload.id,
        x,
        y,
        payload.elevation,
        payload.minLevel,
        payload.maxLevel,
        payload.initialLevel,
      );
      // Only runs on success:
      setPendingCreateNode(null);
      pushUndoEntry(stackKey(project.id, activeScenarioId ?? null), {
        label: `Added ${payload.id}`,
        undo: { deletes: [{ kind: payload.kind, id: payload.id }] },
        redo: {
          recreates: [
            {
              elementType: "node",
              kind: payload.kind,
              id: payload.id,
              // Redo recreates through the same source-CRS coordinate store,
              // so it records the converted values, not raw WGS84.
              x,
              y,
              elevation: payload.elevation,
              ...(payload.kind === "tank"
                ? {
                    minLevel: payload.minLevel,
                    maxLevel: payload.maxLevel,
                    initialLevel: payload.initialLevel,
                  }
                : null),
              patches: [],
            },
          ],
        },
      });
      await saveProjectOnDisk(project.id, activeScenarioId);
      markEdited(project.id, activeScenarioId);
      // No bumpNetwork(): backend event already bumps (see handleNodeMoved).
    },
    [pendingCreateNode, project, activeScenarioId, markEdited, sourceCrs],
  );

  const handleConfirmCreateLink = useCallback(
    async (kind: string, id: string) => {
      if (!pendingCreateLink || !project) return;
      const { fromId, toId } = pendingCreateLink;
      // Throws on backend error — the modal catches and stays open with the error message.
      await createLink(kind, id, fromId, toId);
      // Only runs on success:
      setPendingCreateLink(null);
      pushUndoEntry(stackKey(project.id, activeScenarioId ?? null), {
        label: `Added ${id}`,
        undo: { deletes: [{ kind, id }] },
        redo: {
          recreates: [
            { elementType: "link", kind, id, fromId, toId, patches: [] },
          ],
        },
      });
      await saveProjectOnDisk(project.id, activeScenarioId);
      markEdited(project.id, activeScenarioId);
      // No bumpNetwork(): backend event already bumps (see handleNodeMoved).
    },
    [pendingCreateLink, project, activeScenarioId, markEdited],
  );

  // Compute measure distance: geo in map mode, pixel-scaled in schematic.
  const measureDistanceM = useMemo(() => {
    if (measurePoints.length === 2) {
      const [a, b] = measurePoints.map((p) => ({
        lng: p.position[0],
        lat: p.position[1],
      }));
      return haversineMeters(a.lng, a.lat, b.lng, b.lat);
    }
    return null;
  }, [measurePoints]);

  // Global click-outside: close any open toolbar dropdown when the user clicks
  // anywhere outside the toolbar.
  useEffect(() => {
    if (!showBasemapDropdown) return;
    function onDown(e: PointerEvent) {
      const target = e.target as HTMLElement | null;
      if (target?.closest("[data-toolbar-dropdown]")) return;
      setShowBasemapDropdown(false);
    }
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  }, [showBasemapDropdown]);

  return (
    // Scrub-position plumbing for the inspector's TimeSeriesCard markers.
    // The provider value is a primitive, and CanvasView already re-renders
    // wholly per scrub (currentHour is local state), so this adds no extra
    // re-render surface beyond the card that consumes it.
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        minHeight: 0,
        position: "relative",
      }}
    >
      {/* Main row: canvas + optional results panel */}
      <div
        style={{
          flex: 1,
          display: "flex",
          overflow: "hidden",
          minHeight: 0,
          position: "relative",
        }}
      >
        {/* Canvas area */}
        {/* `background` is undefined while the preference tracks the theme,
            leaving `.canvas-bg` to answer — the stylesheet re-answers on a
            theme change, including one the OS makes while the app is open,
            which nothing here would hear about. */}
        <div
          className="canvas-bg"
          style={{
            flex: 1,
            position: "relative",
            overflow: "hidden",
            background: canvasBackgroundStyle(canvasBackground),
          }}
        >
          {/* Map + Schematic — MapLibre GL JS + deck.gl. Held back until
                prefsReady so MapLibre never initialises with the placeholder
                basemap/CRS (see the cold-load gate above). */}
          {prefsReady && (
            <CanvasErrorBoundary>
              <MapCanvas
                nodes={shownNodes}
                links={shownLinks}
                regions={shownRegions}
                couplings={inletCouplings}
                periodResult={currentPeriodResult}
                generic={genericCanvas}
                isActive={canvasIsActive}
                viewMode={localGrid ? "schematic" : viewMode}
                kindRoles={kindRoles}
                topological={viewMode === "schematic"}
                couplingsResolved={couplingsResolved}
                // The slider carries a track position; the layout wants per-axis
                // multipliers. Converting here keeps the geometric mapping in
                // one place instead of duplicating it in the canvas.
                schematicScale={schematicScale}
                nodeScale={nodeScale}
                nodeVar={nodeVar}
                linkVar={linkVar}
                animateLinks={animateLinks}
                basemap={basemap}
                basemapOpacity={basemapOpacity}
                selectedNodeId={selectedNodeId}
                onSelectNode={handleSelectNode}
                selectedLinkId={selectedLinkId}
                onSelectLink={handleSelectLink}
                selectedRegionId={selectedRegionId}
                onSelectRegion={selectRegion}
                headMin={wdsRanges.head.min}
                headMax={wdsRanges.head.max}
                demandMin={wdsRanges.demand.min}
                demandMax={wdsRanges.demand.max}
                flowMax={wdsRanges.flow.max}
                qualityMin={wdsRanges.quality.min}
                qualityMax={wdsRanges.quality.max}
                pressureMin={wdsRanges.pressure.min}
                pressureMax={wdsRanges.pressure.max}
                velocityMax={wdsRanges.velocity.max}
                colorMode={scaleMode === "criteria" ? "threshold" : "relative"}
                pressureThresholds={thresholds.pressure}
                velocityThresholds={thresholds.velocity}
                flowThresholds={thresholds.flow}
                tool={activeTool}
                onNodeMoved={handleNodeMoved}
                onCreateNodeRequest={handleCreateNodeRequest}
                onCreateLinkRequest={handleCreateLinkRequest}
                onMeasurePoint={handleMeasurePoint}
                measurePoints={measurePointPositions}
                flyToNodeId={flyToState.nodeId}
                flyToLinkId={flyToState.linkId}
                flyToRegionId={flyToState.regionId}
                flyToKey={flyToState.key}
                fitKey={mapFitKey}
                onUserMovedViewport={markViewportUserOwned}
                zoomInKey={zoomInKey}
                zoomOutKey={zoomOutKey}
                resetNorthKey={resetNorthKey}
              />
            </CanvasErrorBoundary>
          )}

          {/* Legend — visible only when simulation results exist. One
                component for every engine: it renders whatever the engine's
                §6 catalog declares, and the per-engine affordances below
                are passed in rather than branched on. */}
          {!!stableResultMeta && genericMeta && (
            <GenericLegend
              meta={genericMeta}
              hasRegions={canvasRegions.length > 0}
              selection={genericSelection}
              onSelect={handleGenericSelect}
              onEditCriteria={() => setCriteriaOpen(true)}
              criteriaEditorOpen={criteriaOpen}
              scaleMode={scaleMode}
              multiStep={!isSteadyState}
              onScaleModeChange={setScaleMode}
              effectiveRanges={{
                // The catalog path carries its own rescaled variable; wds
                // scales through the props above, so its numbers come from
                // the same derivation the map is painted with.
                point:
                  genericCanvas?.node?.variable ??
                  wdsRangeFor(genericSelection.point || nodeVar),
                polyline:
                  genericCanvas?.link?.variable ??
                  wdsRangeFor(genericSelection.polyline || linkVar),
                region: genericCanvas?.region?.variable,
              }}
              criteriaVariables={criteriaVariables}
              criteriaAnnotation={criteriaAnnotation}
              // The verdict's colours describe the map only while the map
              // is showing the verdict; in the data-range modes these
              // variables are painted as plain magnitudes.
              bandColors={scaleMode === "criteria" ? wdsBandColors : () => null}
              onLocateExtreme={
                currentPeriodResult ? handleLocateExtreme : undefined
              }
              animation={legendAnimation}
              detailsOpen={legendOpen}
              onDetailsOpenChange={setLegendOpen}
            />
          )}

          {/* Deliberately not a modal: a backdrop would dim the map, and
              seeing the map is the whole reason for editing criteria from
              here rather than from Analysis. */}
          {criteriaOpen && (
            <div
              className="legend-glass legend-glass--raised"
              role="dialog"
              aria-label="Criteria"
              style={CRITERIA_PANEL_STYLE}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  marginBottom: 10,
                }}
              >
                <span
                  style={{
                    fontSize: "var(--text-sm)",
                    fontWeight: 600,
                    color: "var(--text-primary)",
                    fontFamily: "var(--font-ui)",
                  }}
                >
                  Criteria
                </span>
                <span
                  style={{
                    marginLeft: 8,
                    fontSize: "var(--text-xs)",
                    color: "var(--text-tertiary)",
                    fontFamily: "var(--font-ui)",
                  }}
                >
                  {criteriaSaved ? "Saved" : "Saving…"}
                </span>
                <button
                  type="button"
                  className="tool-btn"
                  aria-label="Close criteria"
                  onClick={() => setCriteriaOpen(false)}
                  style={{
                    marginLeft: "auto",
                    background: "transparent",
                    border: "1px solid transparent",
                    color: "var(--text-tertiary)",
                    cursor: "pointer",
                    borderRadius: 5,
                    padding: "2px 6px",
                    fontSize: "var(--text-sm)",
                  }}
                >
                  ✕
                </button>
              </div>
              <CriteriaEditor criteria={criteria} onChange={setCriteria} />
            </div>
          )}

          {/* Topology-stale notice — results exist but are hidden because
                the network's structure changed since they were produced. */}
          {resultsTopologyStale &&
            !!stableResultMeta &&
            !staleNoticeDismissed && (
              <div
                style={{
                  position: "absolute",
                  top: 60,
                  left: "50%",
                  transform: "translateX(-50%)",
                  zIndex: 25,
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                  padding: "6px 10px",
                  background: "var(--bg-card)",
                  border: "1px solid var(--border)",
                  borderRadius: 8,
                  boxShadow: "var(--shadow-2)",
                }}
              >
                <span
                  style={{
                    fontSize: "var(--text-md)",
                    color: "var(--text-secondary)",
                    fontFamily: "var(--font-ui)",
                  }}
                >
                  Results predate the current network topology — re-run the
                  simulation
                </span>
                <button
                  type="button"
                  onClick={() => setStaleNoticeDismissed(true)}
                  aria-label="Dismiss stale-results notice"
                  style={{
                    border: "none",
                    background: "transparent",
                    cursor: "pointer",
                    display: "inline-flex",
                    padding: 2,
                    color: "var(--text-tertiary)",
                  }}
                >
                  <XMarkIcon style={{ width: 12, height: 12 }} />
                </button>
              </div>
            )}

          {/* Comparison notice — baseline missing results / topology drift.
                Suppressed while the topology-stale notice occupies the same
                slot (that notice already explains the hidden results). */}

          {/* CRS alert — map mode only, shown when coordinates can't be
                reprojected. Suppressed while a catalog proj4 def is still being
                fetched for a persisted CRS (avoids a spurious cold-start flash). */}
          {prefsReady && viewMode === "map" && crsError && !crsResolving && (
            <InvalidCrsOverlay onSetCrs={openCrsModal} />
          )}

          {/* Toolbar overlay — left offset tracks the floating rail width */}
          <CanvasToolbar
            editable={modelEditable}
            viewMode={viewMode}
            localGrid={localGrid}
            canvasBackground={canvasBackground}
            onCanvasBackgroundChange={setCanvasBackground}
            onViewModeChange={setViewMode}
            coordStatus={coordStatus}
            coordMissingCount={coordMissingCount}
            coordTotalCount={rawPositionNodes.length}
            basemap={basemap}
            onBasemapChange={setBasemap}
            basemapOpacity={basemapOpacity}
            onBasemapOpacityChange={setBasemapOpacity}
            showBasemapDropdown={showBasemapDropdown}
            setShowBasemapDropdown={setShowBasemapDropdown}
            sourceCrs={sourceCrs}
            crsError={crsError}
            onOpenCrsModal={openCrsModal}
            onOpenBasemapProviders={openBasemapProvidersModal}
            activeTool={activeTool}
            onToolChange={setActiveTool}
            measurePoints={measurePoints}
            measureDistanceM={measureDistanceM}
            onClearAnnotations={clearAnnotations}
          />

          {/* Bottom-right control stack. One positioned column so each strip
                sits above the last without an offset derived from its
                neighbour's height, and so the inspector offset is applied once
                for the whole stack rather than per strip. */}
          <div
            style={{
              position: "absolute",
              right: "calc(var(--inspector-effective-w, 0px) + 12px)",
              bottom: 12,
              zIndex: 11,
              display: "flex",
              flexDirection: "column",
              alignItems: "flex-end",
              gap: 8,
            }}
          >
            {/* Schematic only: the geographic layout's spacing is the
                  network's real geometry, not ours to redistribute. */}
            {viewMode === "schematic" && (
              <SchematicAspectSlider
                value={schematicAspect}
                onChange={setSchematicAspect}
              />
            )}
            {/* Both views: how big a node is against the links around it is
                the ratio zoom cannot change, and that is as true of a plan
                as of a schematic. */}
            <NodeSizeSlider value={nodeScale} onChange={setNodeScale} />
            <ViewportControls
              mapOnly={mapOnly}
              onZoomIn={() => {
                markViewportUserOwned();
                setZoomInKey((k) => k + 1);
              }}
              onZoomOut={() => {
                markViewportUserOwned();
                setZoomOutKey((k) => k + 1);
              }}
              onResetNorth={() => {
                markViewportUserOwned();
                setResetNorthKey((k) => k + 1);
              }}
              onFit={() => {
                // A fit hands framing back to the app.
                lastViewportCauseRef.current = "fit";
                setMapFitKey((k) => k + 1);
              }}
              onToggleView={handleToggleView}
              viewAction={viewAction}
            />
          </div>

          {/* Inspector panel — node, link or region detail view. The
              wrapper only carries the entrance flag; it is static, so the
              panels inside still position against the canvas. */}
          <div data-inspector-entering={inspectorEntering ? "true" : "false"}>
            {inspectorView === "node" && stableSelectedNode && (
              <NodeInspector
                node={stableSelectedNode}
                onClose={clearSelection}
                onOpenInEditor={() =>
                  focusInEditor(stableSelectedNode.type, stableSelectedNode.id)
                }
                onZoomTo={() =>
                  setFlyToState((s) => ({
                    nodeId: selectedNodeId,
                    linkId: null,
                    regionId: null,
                    key: s.key + 1,
                  }))
                }
                disableZoomTo={!selectedNodeHasCoordinates}
                // Destructive/edit affordances only for editable engines —
                // both props are optional and the inspector hides the
                // gestures entirely when they are absent.
                onDelete={
                  modelEditable
                    ? () =>
                        setPendingDelete({
                          kind: stableSelectedNode.type,
                          id: stableSelectedNode.id,
                        })
                    : undefined
                }
                onRename={
                  modelEditable
                    ? (newId) =>
                        handleRenameElement(
                          stableSelectedNode.type,
                          stableSelectedNode.id,
                          newId,
                        )
                    : undefined
                }
                onOpenPattern={() => {
                  setProjectView("editor");
                }}
                onLocateRelated={(id) => {
                  if (linkMap.has(id)) followElement("link", id);
                }}
                onLocateRegion={(id) => followElement("region", id)}
                nodeVar={nodeVar}
                ranges={stableResultMeta?.ranges}
                hasSimulation={!!stableResultMeta}
                isTransitioning={!!stableResultMeta && !nodeIsEnriched}
                genericResults={genericNodeResults}
              />
            )}
            {inspectorView === "region" && selectedRegion && (
              <RegionInspector
                region={selectedRegion}
                onClose={clearSelection}
                onZoomTo={() =>
                  setFlyToState((s) => ({
                    nodeId: null,
                    linkId: null,
                    regionId: selectedRegionId,
                    key: s.key + 1,
                  }))
                }
                onLocateOutlet={(id) => {
                  if (nodeMap.has(id)) followElement("node", id);
                }}
                onOpenInEditor={() =>
                  focusInEditor(selectedRegion.type, selectedRegion.id)
                }
                genericResults={genericRegionResults}
              />
            )}
            {inspectorView === "link" && stableSelectedLink && (
              <LinkInspector
                link={stableSelectedLink}
                onClose={clearSelection}
                onOpenInEditor={() =>
                  focusInEditor(stableSelectedLink.type, stableSelectedLink.id)
                }
                onZoomTo={() =>
                  setFlyToState((s) => ({
                    nodeId: null,
                    linkId: selectedLinkId,
                    regionId: null,
                    key: s.key + 1,
                  }))
                }
                disableZoomTo={!selectedLinkHasCoordinates}
                onDelete={
                  modelEditable
                    ? () =>
                        setPendingDelete({
                          kind: stableSelectedLink.type,
                          id: stableSelectedLink.id,
                        })
                    : undefined
                }
                onRename={
                  modelEditable
                    ? (newId) =>
                        handleRenameElement(
                          stableSelectedLink.type,
                          stableSelectedLink.id,
                          newId,
                        )
                    : undefined
                }
                onLocateNode={(id) => {
                  if (nodeMap.has(id)) followElement("node", id);
                }}
                linkVar={linkVar}
                ranges={stableResultMeta?.ranges}
                hasSimulation={!!stableResultMeta}
                isTransitioning={!!stableResultMeta && !linkIsEnriched}
                genericResults={genericLinkResults}
              />
            )}
          </div>
        </div>

        {/* Results panel — moved to Results top-level tab */}
      </div>

      {/* Timeline bar — always shown in canvas mode. */}
      {stableResultMeta ? (
        <Timeline
          currentHour={currentHour}
          setCurrentHour={setCurrentHour}
          isPlaying={isPlaying}
          setIsPlaying={setIsPlaying}
          speed={speed}
          setSpeed={setSpeed}
          loop={loop}
          setLoop={setLoop}
          resultMeta={stableResultMeta}
          maxStep={maxStep}
          steadyState={isSteadyState}
        />
      ) : (
        <div
          className="timeline-bar"
          style={{ justifyContent: "center", gap: 8 }}
        >
          <span
            style={{
              color: "var(--text-tertiary)",
              fontSize: "var(--text-md)",
            }}
          >
            {resultMetaLoading
              ? "Loading simulation state..."
              : isSteadyState
                ? "This scenario has no steady-state result yet. Run a simulation to generate the snapshot."
                : "This scenario is not simulated yet. Run a simulation to enable timeline stepping."}
          </span>
        </div>
      )}

      <DeleteConfirmModal
        open={!!pendingDelete}
        elementKind={pendingDelete?.kind ?? ""}
        elementId={pendingDelete?.id ?? ""}
        onConfirm={handleConfirmDelete}
        onCancel={() => setPendingDelete(null)}
      />
      <CreateNodeModal
        open={!!pendingCreateNode}
        suggestId={suggestNodeId}
        lng={pendingCreateNode?.lng ?? 0}
        lat={pendingCreateNode?.lat ?? 0}
        onConfirm={handleConfirmCreateNode}
        onCancel={() => setPendingCreateNode(null)}
      />
      <CreateLinkModal
        open={!!pendingCreateLink}
        suggestId={suggestLinkId}
        fromNodeId={pendingCreateLink?.fromId ?? ""}
        toNodeId={pendingCreateLink?.toId ?? ""}
        onConfirm={handleConfirmCreateLink}
        onCancel={() => setPendingCreateLink(null)}
      />
    </div>
  );
}
