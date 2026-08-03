import { XMarkIcon } from "@heroicons/react/16/solid";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useActiveProject, useAppState, useSimulation } from "../../AppContext";
import {
  type BasemapId,
  clampBasemapOpacity,
  isValidBasemapId,
} from "../../canvas/Basemap";
import { haversineMeters, wgs84ToSourceCrs } from "../../canvas/coords";
import {
  type GenericClassKey,
  GenericLegend,
  type GenericSelection,
} from "../../canvas/GenericLegend";
import { Legend, type LegendThresholds } from "../../canvas/Legend";
import { MapCanvas } from "../../canvas/MapCanvas";
import type { MeasurePoint } from "../../canvas/measureSnap";
import { CurrentPeriodProvider } from "../../canvas/period-context";
import {
  ASPECT_SLIDER_DEFAULT,
  aspectScales,
  clampSliderValue,
} from "../../canvas/schematicAspect";
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
import {
  LinkInspector,
  NodeInspector,
} from "../../components/panels/ElementInspector";
import { engineComponents } from "../../engine/registry";
import {
  createLink,
  createNode,
  deleteElement,
  type GenericPeriodValues,
  type GenericVariable,
  getGenericPeriodValues,
  getPeriodResults,
  type PeriodResults,
  patchNodePosition,
  saveProjectOnDisk,
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
import { CanvasErrorBoundary } from "./CanvasView/CanvasErrorBoundary";
import { CanvasToolbar } from "./CanvasView/CanvasToolbar";
import { InvalidCrsOverlay } from "./CanvasView/InvalidCrsOverlay";
import { SchematicAspectSlider } from "./CanvasView/SchematicAspectSlider";
import { useCrsReprojection } from "./CanvasView/useCrsReprojection";
import { ViewportControls } from "./CanvasView/ViewportControls";

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
  colorMode: "relative" | "threshold";
  /** Schematic layout aspect slider position. Missing in older prefs →
   * defaults to the midpoint, i.e. the layout's native 120:80 spacing. */
  schematicAspect: number;
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
  colorMode: "relative",
  schematicAspect: ASPECT_SLIDER_DEFAULT,
};

// Allowlists so corrupt/stale localStorage can never inject invalid state.
// (Basemap ids are validated structurally via isValidBasemapId instead — the
// provider catalog is open-ended.)
const PREF_VIEW_MODES: readonly ViewMode[] = ["map", "schematic"];
const PREF_NODE_VARS: readonly NodeVariable[] = [
  "pressure",
  "head",
  "demand",
  "quality",
];
const PREF_LINK_VARS: readonly LinkVariable[] = [
  "flow",
  "velocity",
  "status",
  "headloss",
  "quality",
];
const PREF_COLOR_MODES: readonly CanvasPrefs["colorMode"][] = [
  "relative",
  "threshold",
];

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
    commandPaletteOpen,
    showToast,
  } = useAppState();
  const { project, engine } = useActiveProject();
  // Editing affordances exist only for engines whose model this GUI edits;
  // for read-only engines the tools hide rather than refuse per gesture.
  const modelEditable = engineComponents(engine?.key).modelEditable;
  const { markEdited } = useNetworkVersion();
  const renameElementFlow = useElementRename();
  const simParams = useSimParams(project?.id);
  const {
    selectedNodeId,
    selectedLinkId,
    inspectorView,
    selectNode,
    selectLink,
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
  const [nodeVar, setNodeVar] = useState<NodeVariable>(
    CANVAS_PREF_DEFAULTS.nodeVar,
  );
  const [linkVar, setLinkVar] = useState<LinkVariable>(
    CANVAS_PREF_DEFAULTS.linkVar,
  );
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
    key: number;
  }>({ nodeId: null, linkId: null, key: 0 });

  // Register zoom callbacks into the selection context so siblings (e.g. the
  // rail's network list) can trigger canvas fly-to without prop drilling.
  useEffect(() => {
    setZoomCallbacks(
      (id) =>
        setFlyToState((s) => ({ nodeId: id, linkId: null, key: s.key + 1 })),
      (id) =>
        setFlyToState((s) => ({ nodeId: null, linkId: id, key: s.key + 1 })),
    );
  }, [setZoomCallbacks]);
  // ── Colour scale mode and per-variable thresholds ─────────────────────────
  const [colorMode, setColorMode] = useState<"relative" | "threshold">(
    CANVAS_PREF_DEFAULTS.colorMode,
  );
  // Threshold defaults — seeded from SimulationOptions when loaded; user can still adjust.
  // Threshold bands come from the project's criteria file. Previously they
  // were component state, so velocity and flow carried across project
  // switches — the canvas coloured one network against another's bands.
  const {
    criteria,
    setCriteria,
    saved: criteriaSaved,
  } = useProjectCriteria(project?.id ?? null);
  const thresholds: LegendThresholds = useMemo(
    () => ({
      pressure: criteria.pressure,
      velocity: criteria.velocity,
      flow: criteria.flow,
    }),
    [criteria],
  );
  const setThresholds = useCallback(
    (next: LegendThresholds) => {
      setCriteria({ ...criteria, ...next });
    },
    [criteria, setCriteria],
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
    setNodeVar(pick("nodeVar", (v) => PREF_NODE_VARS.includes(v)));
    setLinkVar(pick("linkVar", (v) => PREF_LINK_VARS.includes(v)));
    setColorMode(pick("colorMode", (v) => PREF_COLOR_MODES.includes(v)));
    // Clamp already maps missing/corrupt values to the default.
    setSchematicAspect(clampSliderValue(prefs?.schematicAspect ?? Number.NaN));
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
      colorMode,
      schematicAspect,
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
    colorMode,
    schematicAspect,
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
        e as CustomEvent<"zoom-in" | "zoom-out" | "fit" | "reset-north">
      ).detail;
      if (cmd === "zoom-in") {
        setZoomInKey((k) => k + 1);
      } else if (cmd === "zoom-out") {
        setZoomOutKey((k) => k + 1);
      } else if (cmd === "fit") {
        setMapFitKey((k) => k + 1);
      } else if (cmd === "reset-north") {
        setResetNorthKey((k) => k + 1);
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
  const [genericSelection, setGenericSelection] = useState<GenericSelection>({
    point: "",
    polyline: "",
    region: "",
  });
  const handleGenericSelect = useCallback(
    (cls: GenericClassKey, id: string) =>
      setGenericSelection((s) => ({ ...s, [cls]: id })),
    [],
  );
  const genericCanvas = useMemo<GenericCanvasResults | null>(() => {
    if (!genericMeta) return null;
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
      return { variable: vars[i], values: arrays?.[i] ?? null };
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
  }, [genericMeta, fetchedGenericValues, genericSelection]);

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
    if (resultMeta?.hasPeriodData === false) {
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
    if (resultMeta?.generic) {
      // Catalog-keyed engine: same command, generic decoder. The wds arrays
      // stay null so the canvas renders through the generic channels only.
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
    resultMeta?.hasPeriodData,
    resultMeta?.generic,
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
  const [speed, setSpeed] = useState(1); // 0.5 / 1 / 2 / 4 / 8 ×
  const [loop, setLoop] = useState(true);

  // `maxStep` is the last valid step index: 0..maxStep.
  // Derived from stableResultMeta when available (covers multi-period results),
  // with a fallback for when no simulation has run yet.
  const maxStep = stableResultMeta ? stableResultMeta.times.length - 1 : 24;

  // The "quality" node variable is only offered when the loaded result has
  // quality data; switching to a scenario without it left the picker stuck on
  // a removed option and every junction rendered the null-quality grey.
  const qualityMode = stableResultMeta?.qualityMode ?? "none";
  useEffect(() => {
    if (qualityMode === "none") {
      setNodeVar((v) => (v === "quality" ? "pressure" : v));
      // Same gating for the link quality variable.
      setLinkVar((v) => (v === "quality" ? "velocity" : v));
    }
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

  // Auto-advance the playhead. 1× = 800 ms / step.
  useEffect(() => {
    if (!isPlaying) return;
    const intervalMs = 800 / speed;
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
          if (viewMode === "map") {
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
          if (viewMode === "map" && modelEditable) setActiveTool("edit");
          break;
        case "n":
        case "N":
          if (viewMode === "map" && modelEditable) setActiveTool("add-node");
          break;
        // Not map-gated: creating a link writes only its two node ids.
        case "l":
        case "L":
          if (modelEditable) setActiveTool("add-link");
          break;
        case "Escape":
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
  }, [clearAnnotations, maxStep, projectView, viewMode, modelEditable]);

  const baseNodes = useNodes();
  const baseLinks = useLinks();
  const baseRegions = useRegions();

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
      pressure: currentPeriodResult.nodePressure[i],
      demand: currentPeriodResult.nodeDemand[i],
      head: currentPeriodResult.nodeHead[i],
      quality: currentPeriodResult.nodeQuality?.[i] ?? null,
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
      velocity: currentPeriodResult.linkVelocity[i],
      flow: currentPeriodResult.linkFlow[i],
      status: currentPeriodResult.linkStatus[i],
      quality: currentPeriodResult.linkQuality?.[i] ?? null,
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
      unit: v.unit,
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
      unit: v.unit,
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
  const RAIL_RESULT_COLUMNS = 3;
  const railNodeColumns = useMemo(
    () =>
      (genericMeta?.pointVars ?? []).slice(0, RAIL_RESULT_COLUMNS).map((v) => ({
        key: v.id,
        label: v.label,
        symbol: v.symbol,
        unit: v.unit,
      })),
    [genericMeta],
  );
  const railLinkColumns = useMemo(
    () =>
      (genericMeta?.polylineVars ?? [])
        .slice(0, RAIL_RESULT_COLUMNS)
        .map((v) => ({
          key: v.id,
          label: v.label,
          symbol: v.symbol,
          unit: v.unit,
        })),
    [genericMeta],
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
      railNodeColumns.forEach((c, vi) => {
        const v = arrays[vi]?.[i];
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
      railLinkColumns.forEach((c, vi) => {
        const v = arrays[vi]?.[i];
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

  useEffect(() => {
    setSimData(
      railNodes,
      railLinks,
      genericMeta ? { node: railNodeColumns, link: railLinkColumns } : null,
    );
  }, [
    railNodes,
    railLinks,
    railNodeColumns,
    railLinkColumns,
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
          setFlyToState((s) => ({ nodeId: id, linkId: null, key: s.key + 1 }));
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
          setFlyToState((s) => ({ nodeId: null, linkId: id, key: s.key + 1 }));
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

  // MapCanvas gets the *stable* position/base arrays plus the flat period
  // result — colours update via the periodResult prop without new arrays, so
  // the old flicker-latch over merged arrays is no longer needed. During the
  // brief window after a non-topology edit the previous period result still
  // matches by length and keeps the canvas coloured; after a topology change
  // the length guard in MapCanvas drops stale colours immediately.
  const canvasNodes = posNodes;

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
    (inspectorView === "link" && stableSelectedLink != null);
  useEffect(() => {
    document.documentElement.style.setProperty(
      "--inspector-effective-w",
      inspectorOccupies ? "var(--inspector-w)" : "0px",
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

  // Shared styling for toolbar controls that only work in map mode.
  const mapOnly = viewMode !== "map";

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
    <CurrentPeriodProvider period={currentHour}>
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
          <div
            className="canvas-bg"
            style={{ flex: 1, position: "relative", overflow: "hidden" }}
          >
            {/* Map + Schematic — MapLibre GL JS + deck.gl. Held back until
                prefsReady so MapLibre never initialises with the placeholder
                basemap/CRS (see the cold-load gate above). */}
            {prefsReady && (
              <CanvasErrorBoundary>
                <MapCanvas
                  nodes={canvasNodes}
                  links={canvasLinks}
                  regions={canvasRegions}
                  periodResult={currentPeriodResult}
                  generic={genericCanvas}
                  isActive={canvasIsActive}
                  viewMode={viewMode}
                  // The slider carries a track position; the layout wants per-axis
                  // multipliers. Converting here keeps the geometric mapping in
                  // one place instead of duplicating it in the canvas.
                  schematicScale={schematicScale}
                  nodeVar={nodeVar}
                  linkVar={linkVar}
                  animateLinks={animateLinks}
                  basemap={basemap}
                  basemapOpacity={basemapOpacity}
                  selectedNodeId={selectedNodeId}
                  onSelectNode={handleSelectNode}
                  selectedLinkId={selectedLinkId}
                  onSelectLink={handleSelectLink}
                  headMin={stableResultMeta?.ranges.headMin ?? 0}
                  headMax={stableResultMeta?.ranges.headMax ?? 100}
                  demandMin={stableResultMeta?.ranges.demandMin ?? 0}
                  demandMax={stableResultMeta?.ranges.demandMax ?? 1}
                  flowMax={stableResultMeta?.ranges.flowMax ?? 1}
                  qualityMin={stableResultMeta?.ranges.qualityMin ?? 0}
                  qualityMax={stableResultMeta?.ranges.qualityMax ?? 1}
                  colorMode={colorMode}
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
                  flyToKey={flyToState.key}
                  fitKey={mapFitKey}
                  zoomInKey={zoomInKey}
                  zoomOutKey={zoomOutKey}
                  resetNorthKey={resetNorthKey}
                />
              </CanvasErrorBoundary>
            )}

            {/* Legend — visible only when simulation results exist. The
                engine-generic legend renders the engine's own variable
                catalog; the wds legend keeps its fixed variable set. */}
            {!!stableResultMeta && genericMeta && (
              <GenericLegend
                meta={genericMeta}
                hasRegions={canvasRegions.length > 0}
                selection={genericSelection}
                onSelect={handleGenericSelect}
              />
            )}
            {!!stableResultMeta && !genericMeta && (
              <Legend
                nodeVar={nodeVar}
                setNodeVar={setNodeVar}
                linkVar={linkVar}
                setLinkVar={setLinkVar}
                linkAnimation={linkAnimation}
                setLinkAnimation={setLinkAnimation}
                reducedMotion={reducedMotion}
                qualityMode={stableResultMeta.qualityMode ?? "none"}
                headMin={stableResultMeta.ranges.headMin ?? 0}
                headMax={stableResultMeta.ranges.headMax ?? 100}
                demandMin={stableResultMeta.ranges.demandMin ?? 0}
                demandMax={stableResultMeta.ranges.demandMax ?? 1}
                flowMax={stableResultMeta.ranges.flowMax ?? 1}
                qualityMin={stableResultMeta.ranges.qualityMin ?? 0}
                qualityMax={stableResultMeta.ranges.qualityMax ?? 1}
                colorMode={colorMode}
                thresholds={thresholds}
                onColorModeChange={setColorMode}
                onThresholdsChange={setThresholds}
                onLocateExtreme={
                  currentPeriodResult ? onLocateExtreme : undefined
                }
              />
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
              <ViewportControls
                mapOnly={mapOnly}
                onZoomIn={() => setZoomInKey((k) => k + 1)}
                onZoomOut={() => setZoomOutKey((k) => k + 1)}
                onResetNorth={() => setResetNorthKey((k) => k + 1)}
                onFit={() => setMapFitKey((k) => k + 1)}
              />
            </div>

            {/* Inspector panel — node or link detail view */}
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
                  if (linkMap.has(id)) selectLink(id);
                }}
                nodeVar={nodeVar}
                ranges={stableResultMeta?.ranges}
                hasSimulation={!!stableResultMeta}
                isTransitioning={!!stableResultMeta && !nodeIsEnriched}
                genericResults={genericNodeResults}
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
                  if (nodeMap.has(id)) selectNode(id);
                }}
                linkVar={linkVar}
                ranges={stableResultMeta?.ranges}
                hasSimulation={!!stableResultMeta}
                isTransitioning={!!stableResultMeta && !linkIsEnriched}
                genericResults={genericLinkResults}
              />
            )}
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
    </CurrentPeriodProvider>
  );
}
