import {
  ArrowsPointingOutIcon,
  ArrowsRightLeftIcon,
  ChevronUpDownIcon,
  CursorArrowRaysIcon,
  EyeIcon,
  LinkIcon,
  MapPinIcon,
  MinusIcon,
  PencilSquareIcon,
  PlusIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import {
  type CSSProperties,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useActiveProject, useAppState, useSimulation } from "../../AppContext";
import { AnnotationSummary, MeasureOverlay } from "../../canvas/Annotations";
import type { BasemapStyle } from "../../canvas/Basemap";
import {
  haversineMeters,
  pickCoordSample,
  reprojectLinkVerticesCached,
  reprojectNodesCached,
  setPendingCrsSuggestionSample,
} from "../../canvas/coords";
import { Legend, type LegendThresholds } from "../../canvas/Legend";
import { useCanvasLayers } from "../../canvas/layers-context";
import { MapCanvas } from "../../canvas/MapCanvas";
import { useCanvasSelection } from "../../canvas/selection-context";
import { useCanvasStatus } from "../../canvas/status-context";
import { Timeline } from "../../canvas/Timeline";
import type {
  CanvasTool,
  ClickPoint,
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
import {
  createLink,
  createNode,
  deleteElement,
  getPeriodResults,
  type PeriodResults,
  patchNodePosition,
  saveProjectOnDisk,
  useLinks,
  useNodes,
  useSimParams,
} from "../../hooks";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";
import { useReducedMotion } from "../../hooks/useReducedMotion";
import { CanvasErrorBoundary } from "./CanvasView/CanvasErrorBoundary";
import { CoordStatusIndicator } from "./CanvasView/CoordStatusIndicator";

const NODE_KIND_PREFIX: Record<string, string> = {
  junction: "J",
  reservoir: "R",
  tank: "T",
};

/** Centred inline-flex layout shared by all icon toolbar buttons. */
const ICON_BTN_STYLE: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
};

/** Standard 14px toolbar icon size. */
const ICON_14: CSSProperties = { width: 14, height: 14 };

/** Display label for a basemap style value. */
const basemapLabel = (b: BasemapStyle) =>
  b === "none" ? "No basemap" : b.charAt(0).toUpperCase() + b.slice(1);

// ── Per-project canvas prefs ────────────────────────────────────────────────
// Persisted under one JSON key per project (unlike hydra2-link-animation,
// which is deliberately a global preference and stays untouched).
const canvasPrefsKey = (projectId: string) =>
  `hydra2-canvas-prefs:${projectId}`;

interface CanvasPrefs {
  viewMode: ViewMode;
  basemap: BasemapStyle;
  nodeVar: NodeVariable;
  linkVar: LinkVariable;
  colorMode: "relative" | "threshold";
}

// Allowlists so corrupt/stale localStorage can never inject invalid state.
const PREF_VIEW_MODES: readonly ViewMode[] = ["map", "schematic"];
const PREF_BASEMAPS: readonly BasemapStyle[] = [
  "streets",
  "light",
  "dark",
  "none",
];
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
    openCrsModal,
    setProjectView,
    projectView,
    railOpen,
    commandPaletteOpen,
  } = useAppState();
  const { project } = useActiveProject();
  const { markEdited } = useNetworkVersion();
  const simParams = useSimParams(project?.id);
  const { layers: canvasLayers, setLayer } = useCanvasLayers();
  const { setCoordStatus } = useCanvasStatus();
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
  const [currentHour, setCurrentHour] = useState(0);
  const [nodeVar, setNodeVar] = useState<NodeVariable>("pressure");
  const [linkVar, setLinkVar] = useState<LinkVariable>("velocity");
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
    "relative",
  );
  // Threshold defaults — seeded from SimulationOptions when loaded; user can still adjust.
  const [thresholds, setThresholds] = useState<LegendThresholds>({
    pressure: { low: 24, required: 35, high: 45 }, // m
    velocity: { low: 0.1, target: 0.5, high: 1.5 }, // m/s
    flow: { low: 0.1, target: 1.0, high: 10.0 }, // L/s
  });
  // Seed pressure thresholds from SimulationOptions when they first arrive.
  useEffect(() => {
    if (!simParams) return;
    const min = simParams.pdaMinPressure;
    const req =
      simParams.pdaRequiredPressure > min
        ? simParams.pdaRequiredPressure
        : min + 11;
    setThresholds((prev) => ({
      ...prev,
      pressure: { low: min, required: req, high: req + 10 },
    }));
  }, [simParams]);
  // ── View mode (Map vs Schematic) and basemap style ───────────────────
  // "none" is a *map* basemap (geographic layout, no tiles), distinct from
  // schematic mode (idealised orthogonal layout).
  const [viewMode, setViewMode] = useState<ViewMode>("map");
  const [basemap, setBasemap] = useState<BasemapStyle>("streets");

  // ── Per-project canvas prefs: restore on project switch, persist on change.
  // `prefsLoadedFor` gates persisting so the write effect (which also re-runs
  // on project switch) can never store the previous project's values under
  // the new project's key before the restore has been applied.
  const [prefsLoadedFor, setPrefsLoadedFor] = useState<string | null>(null);
  useEffect(() => {
    const id = project?.id;
    if (!id) return;
    const prefs = readCanvasPrefs(id);
    if (prefs) {
      if (prefs.viewMode && PREF_VIEW_MODES.includes(prefs.viewMode)) {
        setViewMode(prefs.viewMode);
      }
      if (prefs.basemap && PREF_BASEMAPS.includes(prefs.basemap)) {
        setBasemap(prefs.basemap);
      }
      if (prefs.nodeVar && PREF_NODE_VARS.includes(prefs.nodeVar)) {
        setNodeVar(prefs.nodeVar);
      }
      if (prefs.linkVar && PREF_LINK_VARS.includes(prefs.linkVar)) {
        setLinkVar(prefs.linkVar);
      }
      if (prefs.colorMode && PREF_COLOR_MODES.includes(prefs.colorMode)) {
        setColorMode(prefs.colorMode);
      }
    }
    setPrefsLoadedFor(id);
  }, [project?.id]);
  useEffect(() => {
    const id = project?.id;
    if (!id || prefsLoadedFor !== id) return;
    const prefs: CanvasPrefs = {
      viewMode,
      basemap,
      nodeVar,
      linkVar,
      colorMode,
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
    nodeVar,
    linkVar,
    colorMode,
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
  // In map mode: geo points { lng, lat }. In schematic mode: SVG ClickPoints.
  const [measureGeoPts, setMeasureGeoPts] = useState<
    { lng: number; lat: number }[]
  >([]);
  const [measurePts, setMeasurePts] = useState<ClickPoint[]>([]);
  const svgRef = useRef<SVGSVGElement | null>(null);

  /** Discard measure points in both coordinate spaces. */
  const clearAnnotations = useCallback(() => {
    setMeasurePts([]);
    setMeasureGeoPts([]);
  }, []);

  useEffect(() => {
    function onToolCommand(e: Event) {
      const tool = (e as CustomEvent<CanvasTool>).detail;
      if (tool === "measure") clearAnnotations();
      setActiveTool(tool);
    }
    window.addEventListener("hydra:canvas-tool", onToolCommand);
    return () => window.removeEventListener("hydra:canvas-tool", onToolCommand);
  }, [clearAnnotations]);

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
  const { resultMeta, resultMetaLoading, resultGeneration } = useSimulation();
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
  const [currentPeriodResult, setCurrentPeriodResult] =
    useState<PeriodResults | null>(null);

  // On project or scenario change, discard stale period results immediately.
  // This guarantees overlays never show data from a previously active scenario.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `project?.id` and `activeScenarioId` are intentional triggers to discard stale period data on switch.
  useEffect(() => {
    setCurrentPeriodResult(null);
  }, [project?.id, activeScenarioId]);

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
    if (resultMetaKey == null || !project?.id) {
      // No simulation exists for this scenario — discard any stale period result
      // so the canvas and inspector show the "no results" state.
      setCurrentPeriodResult(null);
      return;
    }
    let cancelled = false;
    // Clamp: on switching to a shorter result set this effect can run before
    // the playhead-clamp effect corrects currentHour, and an out-of-range
    // period would surface a spurious backend error.
    const period = Math.max(
      0,
      Math.min(currentHour, (resultMeta?.times.length ?? 1) - 1),
    );
    getPeriodResults(project.id, period, activeScenarioId)
      .then((r) => {
        if (!cancelled) {
          setCurrentPeriodResult(r);
        }
      })
      // Decode failures reject (already console.error'd in getPeriodResults);
      // keep the previous period visible rather than crashing the effect.
      .catch(() => {});
    return () => {
      cancelled = true;
    };
  }, [
    project?.id,
    currentHour,
    resultMetaKey,
    activeScenarioId,
    resultMeta?.times.length,
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
          setActiveTool("measure");
          clearAnnotations();
          break;
        case "e":
        case "E":
          setActiveTool("edit");
          break;
        case "n":
        case "N":
          setActiveTool("add-node");
          break;
        case "l":
        case "L":
          setActiveTool("add-link");
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
  }, [clearAnnotations, maxStep, projectView]);

  const baseNodes = useNodes();
  const baseLinks = useLinks();
  // ── CRS reprojection ────────────────────────────────────────────────────
  // EPANET [COORDINATES] carry no CRS tag. We default to WGS84 (pass-through).
  // When the user sets a different source CRS we reproject the raw x/y to
  // WGS84 before passing them to MapCanvas. Schematic mode uses the BFS layout
  // so it is never affected by CRS — only geo/map mode needs this.
  // Initialise from the persisted project value so the reprojection survives
  // session restarts. Falls back to WGS84 for draft projects (project is null).
  const [sourceCrs, setSourceCrs] = useState<string>(
    project?.sourceCrs ?? "EPSG:4326",
  );
  const [crsError, setCrsError] = useState<string | null>(null);

  // Keep sourceCrs in sync if the project row changes (e.g. loaded from disk
  // while the canvas is already open).
  useEffect(() => {
    setSourceCrs(project?.sourceCrs ?? "EPSG:4326");
  }, [project?.sourceCrs]);

  // Raw positional nodes (no pressure/demand merged yet) used for CRS sniffing
  // and reprojection. Stable across timeline scrubs.
  const rawPositionNodes = baseNodes;

  // Clear any prior reprojection error when the network identity changes
  // (new load or project switch); the reprojection memo below re-derives it.
  useEffect(() => {
    if (rawPositionNodes.length === 0) return;
    setCrsError(null);
  }, [rawPositionNodes.length]);

  // Classify how many nodes have real map coordinates.
  // The Rust backend emits (0, 0) for nodes that have no [COORDINATES] entry in
  // the INP — so x === 0 && y === 0 is the sentinel for "missing".
  const coordMissingCount = useMemo(
    () => rawPositionNodes.filter((n) => n.x === 0 && n.y === 0).length,
    [rawPositionNodes],
  );
  const coordStatus = useMemo((): "complete" | "partial" | "empty" => {
    if (rawPositionNodes.length === 0) return "complete"; // nothing loaded yet
    if (coordMissingCount === 0) return "complete";
    if (coordMissingCount === rawPositionNodes.length) return "empty";
    return "partial";
  }, [rawPositionNodes, coordMissingCount]);

  // Push coord status to the shared context so the TopBar breadcrumb indicator
  // can read it without prop-drilling through ProjectPage.
  useEffect(() => {
    setCoordStatus(coordStatus, coordMissingCount, rawPositionNodes.length);
  }, [coordStatus, coordMissingCount, rawPositionNodes.length, setCoordStatus]);

  // Apply reprojection to the raw positional nodes. Result is memo-ised so
  // pressure/velocity scrubs (which don't change x/y) don't re-run proj4.
  // proj4 errors are surfaced via `crsError` (set in the effect below, not
  // here — setting state inside useMemo is a React anti-pattern).
  // Per-node reprojection cache: on a projected CRS every network mutation
  // delivers a fresh baseNodes array, but almost all coordinates are
  // unchanged — reuse both the proj4 result and the output object (identity)
  // for nodes whose source object is identical, so a single-element patch
  // costs one proj4 call instead of 46k.
  const reprojCacheRef = useRef<{
    crs: string;
    byId: Map<
      string,
      {
        src: (typeof baseNodes)[number];
        out: (typeof baseNodes)[number];
      }
    >;
  }>({ crs: "", byId: new Map() });
  const reprojection = useMemo(() => {
    if (sourceCrs === "EPSG:4326") {
      // Even with the default CRS, check that the raw coordinates are within
      // WGS84 range. If any node is out of range the user has projected
      // coordinates and needs to set the source CRS.
      const outOfRange = rawPositionNodes.some(
        (n) =>
          !(n.x === 0 && n.y === 0) &&
          (n.x < -180 || n.x > 180 || n.y < -90 || n.y > 90),
      );
      if (outOfRange) {
        return {
          nodes: rawPositionNodes,
          error:
            "Coordinates are outside WGS84 range. Set the source CRS in the toolbar.",
        } as { nodes: typeof rawPositionNodes; error: string | null };
      }
      return { nodes: rawPositionNodes, error: null as string | null };
    }
    try {
      const cache = reprojCacheRef.current;
      if (cache.crs !== sourceCrs) {
        cache.crs = sourceCrs;
        cache.byId = new Map();
      }
      const nodes = reprojectNodesCached(
        rawPositionNodes,
        sourceCrs,
        cache.byId,
      );
      return { nodes, error: null };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return { nodes: rawPositionNodes, error: msg };
    }
  }, [sourceCrs, rawPositionNodes]);

  // Surface reprojection errors to the toolbar without setting state during
  // render.  Runs after every reprojection result.
  useEffect(() => {
    setCrsError(reprojection.error);
  }, [reprojection.error]);

  const reprojectedPositionNodes = reprojection.nodes;

  // Build a lookup from node id → reprojected [x, y] so allNodes below can
  // merge reprojected positions without touching pressure/demand.
  const reprojectedXY = useMemo(() => {
    const m = new Map<string, { x: number; y: number }>();
    // Identity match means coordinates are already WGS84 — posNodes will
    // return baseNodes untouched, so skip building a 46k-entry Map.
    if (reprojectedPositionNodes === baseNodes) return m;
    for (const n of reprojectedPositionNodes) m.set(n.id, { x: n.x, y: n.y });
    return m;
  }, [reprojectedPositionNodes, baseNodes]);
  // Base nodes with reprojected x/y merged — deliberately independent of
  // period results so its identity is stable across timeline scrubs. The
  // canvas reads sim values from the flat arrays (periodResult prop) instead.
  const posNodes = useMemo(() => {
    // No reprojection ran (EPSG:4326 in range): reuse baseNodes as-is —
    // identity stability here keeps MapCanvas's data memos from rebuilding.
    if (reprojectedXY.size === 0) return baseNodes;
    return baseNodes.map((n) => {
      const pos = reprojectedXY.get(n.id);
      return pos ? { ...n, x: pos.x, y: pos.y } : n;
    });
  }, [baseNodes, reprojectedXY]);

  // O(1) enrichment flag (replaces the previous 46k `.some` scans): true when
  // the current period result matches the network's node order/length.
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

  // Keep the selection context's sim data in sync so the rail can display
  // live result values without re-fetching from the backend. Always push:
  // when the period result matches the network the arrays are merged with sim
  // values; after a topology change they are the fresh raw arrays — holding
  // back in that case left deleted elements listed in the rail forever, since
  // no period refetch arrives until the next run.
  useEffect(() => {
    setSimData(allNodes, allLinks);
  }, [allNodes, allLinks, setSimData]);

  // MapCanvas gets the *stable* position/base arrays plus the flat period
  // result — colours update via the periodResult prop without new arrays, so
  // the old flicker-latch over merged arrays is no longer needed. During the
  // brief window after a non-topology edit the previous period result still
  // matches by length and keeps the canvas coloured; after a topology change
  // the length guard in MapCanvas drops stale colours immediately.
  const canvasNodes = posNodes;
  // Link polyline vertices are stored in the source CRS exactly like node
  // coords, so they go through the same proj4 transform (with the same
  // EPSG:4326 identity fast-path and a per-link identity cache). Errors are
  // already surfaced by the node reprojection above — fall back to the raw
  // links so map+schematic keep rendering.
  const linkReprojCacheRef = useRef<{
    crs: string;
    byId: Map<
      string,
      { src: (typeof baseLinks)[number]; out: (typeof baseLinks)[number] }
    >;
  }>({ crs: "", byId: new Map() });
  const canvasLinks = useMemo(() => {
    if (sourceCrs === "EPSG:4326") return baseLinks;
    try {
      const cache = linkReprojCacheRef.current;
      if (cache.crs !== sourceCrs) {
        cache.crs = sourceCrs;
        cache.byId = new Map();
      }
      return reprojectLinkVerticesCached(baseLinks, sourceCrs, cache.byId);
    } catch {
      return baseLinks;
    }
  }, [sourceCrs, baseLinks]);

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

  // Reset to Select when switching to Schematic if the active tool is map-only.
  useEffect(() => {
    if (
      viewMode === "schematic" &&
      (activeTool === "edit" ||
        activeTool === "add-node" ||
        activeTool === "add-link" ||
        activeTool === "measure")
    ) {
      setActiveTool("select");
    }
  }, [viewMode, activeTool]);

  const svgCursor = activeTool === "measure" ? "crosshair" : "default";
  const canvasIsActive = isActive && projectView === "canvas";

  // Shared styling for toolbar controls that only work in map mode.
  const mapOnly = viewMode !== "map";
  const mapOnlyDim: CSSProperties = {
    opacity: mapOnly ? 0.38 : undefined,
    cursor: mapOnly ? "not-allowed" : undefined,
  };
  const mapOnlyTooltip = (label: string) => (mapOnly ? "Map mode only" : label);

  const handleNodeMoved = useCallback(
    async (id: string, x: number, y: number) => {
      if (!project) return;
      await patchNodePosition(id, x, y);
      await saveProjectOnDisk(project.id, activeScenarioId);
      markEdited(activeScenarioId);
      // No bumpNetwork(): the backend emits `network-changed`, which already
      // bumps the version — a manual bump doubled the full-snapshot refetch.
    },
    [project, activeScenarioId, markEdited],
  );

  const handleConfirmDelete = useCallback(async () => {
    if (!pendingDelete || !project) return;
    const { kind, id } = pendingDelete;
    setPendingDelete(null);
    clearSelection();
    await deleteElement(kind, id);
    await saveProjectOnDisk(project.id, activeScenarioId);
    markEdited(activeScenarioId);
    // No bumpNetwork(): backend event already bumps (see handleNodeMoved).
  }, [pendingDelete, project, activeScenarioId, markEdited, clearSelection]);

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
      // Throws on backend error — the modal catches and stays open with the error message.
      await createNode(
        payload.kind,
        payload.id,
        lng,
        lat,
        payload.elevation,
        payload.minLevel,
        payload.maxLevel,
        payload.initialLevel,
      );
      // Only runs on success:
      setPendingCreateNode(null);
      await saveProjectOnDisk(project.id, activeScenarioId);
      markEdited(activeScenarioId);
      // No bumpNetwork(): backend event already bumps (see handleNodeMoved).
    },
    [pendingCreateNode, project, activeScenarioId, markEdited],
  );

  const handleConfirmCreateLink = useCallback(
    async (kind: string, id: string) => {
      if (!pendingCreateLink || !project) return;
      const { fromId, toId } = pendingCreateLink;
      // Throws on backend error — the modal catches and stays open with the error message.
      await createLink(kind, id, fromId, toId);
      // Only runs on success:
      setPendingCreateLink(null);
      await saveProjectOnDisk(project.id, activeScenarioId);
      markEdited(activeScenarioId);
      // No bumpNetwork(): backend event already bumps (see handleNodeMoved).
    },
    [pendingCreateLink, project, activeScenarioId, markEdited],
  );

  // Compute measure distance: geo in map mode, pixel-scaled in schematic.
  const measureDistanceM = useMemo(() => {
    if (viewMode === "map" && measureGeoPts.length === 2) {
      const [a, b] = measureGeoPts;
      return haversineMeters(a.lng, a.lat, b.lng, b.lat);
    }
    return null;
  }, [viewMode, measureGeoPts]);

  const handleMeasurePoint = useCallback((lng: number, lat: number) => {
    setMeasureGeoPts((prev) => {
      if (prev.length >= 1) {
        // Second click completes the measurement.
        return [prev[0], { lng, lat }];
      }
      // Should not happen (MapCanvas manages first-click anchor),
      // but guard just in case.
      return [{ lng, lat }];
    });
  }, []);
  // Convert a mouse event to SVG user-space coordinates (via the screen CTM)
  // so measure points stay anchored to the network even when the SVG is scaled.
  const eventToSvgPoint = useCallback(
    (e: React.MouseEvent<SVGSVGElement>): ClickPoint | null => {
      const svg = svgRef.current;
      if (!svg) return null;
      const pt = svg.createSVGPoint();
      pt.x = e.clientX;
      pt.y = e.clientY;
      const ctm = svg.getScreenCTM();
      if (!ctm) return null;
      const local = pt.matrixTransform(ctm.inverse());
      return { x: local.x, y: local.y };
    },
    [],
  ); // svgRef is a stable ref

  const handleSvgClick = useCallback(
    (e: React.MouseEvent<SVGSVGElement>) => {
      setShowBasemapDropdown(false);
      if (activeTool === "measure") {
        const p = eventToSvgPoint(e);
        if (!p) return;
        // Measure is two-point. Third click resets and starts a new one.
        setMeasurePts((prev) => (prev.length >= 2 ? [p] : [...prev, p]));
      }
    },
    [activeTool, eventToSvgPoint],
  );

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
          {/* Map + Schematic — MapLibre GL JS + deck.gl */}
          <CanvasErrorBoundary>
            <MapCanvas
              nodes={canvasNodes}
              links={canvasLinks}
              periodResult={currentPeriodResult}
              isActive={canvasIsActive}
              viewMode={viewMode}
              nodeVar={nodeVar}
              linkVar={linkVar}
              animateLinks={animateLinks}
              basemap={basemap}
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
              flyToNodeId={flyToState.nodeId}
              flyToLinkId={flyToState.linkId}
              flyToKey={flyToState.key}
              fitKey={mapFitKey}
              zoomInKey={zoomInKey}
              zoomOutKey={zoomOutKey}
              resetNorthKey={resetNorthKey}
            />
          </CanvasErrorBoundary>

          {/* Legend — visible only when simulation results exist */}
          {!!stableResultMeta && (
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
            />
          )}

          {/* CRS alert — map mode only, shown when coordinates can't be reprojected */}
          {viewMode === "map" && crsError && (
            <div
              style={{
                position: "absolute",
                inset: 0,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                pointerEvents: "none",
              }}
            >
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  alignItems: "center",
                  gap: 10,
                  padding: "24px 28px",
                  background: "var(--bg-card)",
                  border: "1px solid var(--border)",
                  borderRadius: 10,
                  boxShadow: "0 4px 24px rgba(0,0,0,0.18)",
                  maxWidth: 360,
                  textAlign: "center",
                }}
              >
                <span style={{ fontSize: 28 }}>🗺️</span>
                <span
                  style={{
                    fontSize: 14,
                    fontWeight: 600,
                    color: "var(--text-primary)",
                    fontFamily: "var(--font-ui)",
                  }}
                >
                  Invalid coordinate reference system
                </span>
                <span
                  style={{
                    fontSize: 12,
                    color: "var(--text-secondary)",
                    fontFamily: "var(--font-ui)",
                    lineHeight: 1.6,
                  }}
                >
                  Map view requires valid WGS84 coordinates. Set the correct
                  source CRS to reproject the network, or switch to Schematic
                  view.
                </span>
                <div style={{ display: "flex", gap: 8 }}>
                  <button
                    type="button"
                    className="tool-btn"
                    onClick={openCrsModal}
                    style={{
                      pointerEvents: "auto",
                      padding: "0 10px",
                      fontSize: 12,
                    }}
                  >
                    Set source CRS
                  </button>
                  {/* Auto-suggestion only makes sense for the out-of-range
                      case (coords look projected while CRS is the EPSG:4326
                      default) — with a non-default CRS the error is a proj4
                      failure, not a wrong-guess situation. */}
                  {sourceCrs === "EPSG:4326" && (
                    <button
                      type="button"
                      className="tool-btn"
                      onClick={() => {
                        setPendingCrsSuggestionSample(
                          pickCoordSample(rawPositionNodes),
                        );
                        openCrsModal();
                      }}
                      style={{
                        pointerEvents: "auto",
                        padding: "0 10px",
                        fontSize: 12,
                      }}
                    >
                      Suggest CRS…
                    </button>
                  )}
                </div>
              </div>
            </div>
          )}

          {/* Legacy SVG annotation overlays (schematic mode only).
               pointer-events: none — deck.gl handles all interaction. */}
          {viewMode === "schematic" && (
            // biome-ignore lint/a11y/useKeyWithClickEvents: SVG overlay handles pointer measurement gestures.
            <svg
              ref={svgRef}
              width="100%"
              height="100%"
              viewBox="0 0 800 560"
              preserveAspectRatio="xMidYMid meet"
              style={{
                position: "absolute",
                inset: 0,
                pointerEvents: activeTool === "measure" ? "all" : "none",
                cursor: svgCursor,
              }}
              onClick={handleSvgClick}
            >
              <title>Schematic annotations overlay</title>
              {/* Annotation overlays */}
              <MeasureOverlay points={measurePts} />
            </svg>
          )}

          {/* Toolbar overlay — left offset tracks the floating rail width */}
          <div
            style={{
              position: "absolute",
              top: 12,
              left: "calc(var(--rail-effective-w, 0px) + 12px)",
              zIndex: 10,
              transition: "left var(--rail-transition)",
            }}
          >
            <div className="canvas-toolbar">
              {/* ── VIEW MODE TOGGLE ─────────────────────────────────────────── */}
              <div
                style={{
                  display: "flex",
                  background: "var(--bg-card)",
                  border: "1px solid var(--border)",
                  borderRadius: 6,
                  padding: 2,
                  gap: 2,
                  flexShrink: 0,
                }}
              >
                {(["map", "schematic"] as ViewMode[]).map((m) => (
                  <button
                    type="button"
                    key={m}
                    onClick={() => setViewMode(m)}
                    style={{
                      border: "none",
                      background:
                        viewMode === m ? "var(--accent-dim)" : "transparent",
                      color:
                        viewMode === m
                          ? "var(--accent)"
                          : "var(--text-secondary)",
                      padding: "3px 10px",
                      borderRadius: 4,
                      fontSize: 11,
                      fontWeight: 600,
                      cursor: "pointer",
                      fontFamily: "var(--font-ui)",
                      letterSpacing: "0.02em",
                      whiteSpace: "nowrap",
                      flexShrink: 0,
                    }}
                    data-tooltip={
                      m === "map"
                        ? "Geographic layout"
                        : "Idealised orthogonal layout"
                    }
                    data-tooltip-pos="bottom"
                  >
                    {m === "map" ? "Map" : "Schematic"}
                  </button>
                ))}
              </div>

              {/* Coordinate-coverage indicator — only shown when coords are missing */}
              {viewMode === "map" &&
                coordStatus !== "complete" &&
                rawPositionNodes.length > 0 && (
                  <CoordStatusIndicator
                    status={coordStatus}
                    missingCount={coordMissingCount}
                    totalCount={rawPositionNodes.length}
                  />
                )}

              {/* Basemap dropdown */}
              <div
                data-toolbar-dropdown
                style={{ position: "relative", opacity: mapOnlyDim.opacity }}
              >
                <button
                  type="button"
                  className="tool-btn"
                  disabled={mapOnly}
                  style={{
                    width: "auto",
                    padding: "0 8px",
                    fontSize: 12,
                    gap: 4,
                    display: "flex",
                    alignItems: "center",
                    cursor: mapOnlyDim.cursor,
                  }}
                  onClick={(e) => {
                    if (mapOnly) return;
                    e.stopPropagation();
                    setShowBasemapDropdown((v) => !v);
                  }}
                  data-tooltip={mapOnlyTooltip("Basemap")}
                  data-tooltip-pos="bottom"
                >
                  {basemapLabel(basemap)}{" "}
                  <ChevronUpDownIcon
                    style={{ width: 12, height: 12, verticalAlign: "middle" }}
                  />
                </button>
                {showBasemapDropdown && viewMode === "map" && (
                  <div
                    style={{
                      position: "absolute",
                      top: "calc(100% + 4px)",
                      left: 0,
                      background: "var(--bg-panel)",
                      border: "1px solid var(--border)",
                      borderRadius: 7,
                      boxShadow: "var(--shadow-2)",
                      overflow: "hidden",
                      minWidth: 140,
                      zIndex: 20,
                    }}
                  >
                    {(
                      ["streets", "light", "dark", "none"] as BasemapStyle[]
                    ).map((b) => (
                      <button
                        type="button"
                        key={b}
                        onClick={() => {
                          setBasemap(b);
                          setShowBasemapDropdown(false);
                        }}
                        style={{
                          display: "block",
                          width: "100%",
                          padding: "7px 12px",
                          border: "none",
                          background:
                            basemap === b ? "var(--accent-dim)" : "transparent",
                          color:
                            basemap === b
                              ? "var(--accent)"
                              : "var(--text-secondary)",
                          cursor: "pointer",
                          fontSize: 12,
                          textAlign: "left",
                          fontFamily: "var(--font-ui)",
                        }}
                      >
                        {basemapLabel(b)}
                      </button>
                    ))}
                  </div>
                )}
              </div>

              {/* CRS status + modal launcher */}
              <div
                data-toolbar-dropdown
                style={{ position: "relative", opacity: mapOnlyDim.opacity }}
              >
                <button
                  type="button"
                  className="tool-btn"
                  disabled={mapOnly}
                  style={{
                    width: "auto",
                    padding: "0 8px",
                    fontSize: 12,
                    gap: 4,
                    display: "flex",
                    alignItems: "center",
                    cursor: mapOnlyDim.cursor,
                    borderColor:
                      !mapOnly && crsError ? "var(--status-error)" : undefined,
                  }}
                  onClick={(e) => {
                    if (mapOnly) return;
                    e.stopPropagation();
                    setShowBasemapDropdown(false);
                    openCrsModal();
                  }}
                  data-tooltip={mapOnlyTooltip(
                    crsError ?? "Set source coordinate reference system",
                  )}
                  data-tooltip-pos="bottom"
                >
                  {sourceCrs}{" "}
                  <ChevronUpDownIcon
                    style={{ width: 12, height: 12, verticalAlign: "middle" }}
                  />
                </button>
              </div>

              <div className="tool-divider" />

              {/* ── BOTH MODES ───────────────────────────────────────────────── */}

              <button
                type="button"
                className={`tool-btn${activeTool === "select" ? " active" : ""}`}
                onClick={() => setActiveTool("select")}
                data-tooltip="Select (S)"
                data-tooltip-pos="bottom"
                aria-label="Select"
                style={ICON_BTN_STYLE}
              >
                <CursorArrowRaysIcon style={ICON_14} />
              </button>

              <button
                type="button"
                className={`tool-btn${activeTool === "edit" ? " active" : ""}`}
                onClick={() => setActiveTool("edit")}
                disabled={mapOnly}
                data-tooltip={mapOnlyTooltip("Edit / move nodes (E)")}
                data-tooltip-pos="bottom"
                aria-label="Edit"
                style={{ ...ICON_BTN_STYLE, ...mapOnlyDim }}
              >
                <PencilSquareIcon style={ICON_14} />
              </button>

              <button
                type="button"
                className={`tool-btn${activeTool === "add-node" ? " active" : ""}`}
                disabled={mapOnly}
                onClick={() => setActiveTool("add-node")}
                data-tooltip={mapOnlyTooltip("Add node (N)")}
                data-tooltip-pos="bottom"
                aria-label="Add node"
                style={{ ...ICON_BTN_STYLE, ...mapOnlyDim }}
              >
                <MapPinIcon style={ICON_14} />
              </button>

              <button
                type="button"
                className={`tool-btn${activeTool === "add-link" ? " active" : ""}`}
                disabled={mapOnly}
                onClick={() => setActiveTool("add-link")}
                data-tooltip={mapOnlyTooltip("Add link (L)")}
                data-tooltip-pos="bottom"
                aria-label="Add link"
                style={{ ...ICON_BTN_STYLE, ...mapOnlyDim }}
              >
                <LinkIcon style={ICON_14} />
              </button>

              {/* Measure distance */}
              <button
                type="button"
                className={`tool-btn${activeTool === "measure" ? " active" : ""}`}
                disabled={mapOnly}
                onClick={() => {
                  setActiveTool("measure");
                  clearAnnotations();
                }}
                data-tooltip={mapOnlyTooltip("Measure distance (D)")}
                data-tooltip-pos="bottom"
                aria-label="Measure distance"
                style={{
                  fontSize: 12,
                  fontWeight: 600,
                  ...ICON_BTN_STYLE,
                  ...mapOnlyDim,
                }}
              >
                <ArrowsRightLeftIcon style={ICON_14} />
              </button>

              {(measureGeoPts.length > 0 || measurePts.length > 0) &&
                viewMode === "map" && (
                  <button
                    type="button"
                    className="tool-btn"
                    onClick={clearAnnotations}
                    data-tooltip="Clear annotations"
                    data-tooltip-pos="bottom"
                    aria-label="Clear annotations"
                    style={{
                      fontSize: 11,
                      color: "var(--text-tertiary)",
                      ...ICON_BTN_STYLE,
                    }}
                  >
                    <XMarkIcon style={ICON_14} />
                  </button>
                )}

              <div className="tool-divider" />

              {/* Layer visibility toggles */}
              <button
                type="button"
                className={`tool-btn${canvasLayers.model ? " active" : ""}`}
                onClick={() => setLayer("model", !canvasLayers.model)}
                data-tooltip="Toggle base model"
                data-tooltip-pos="bottom"
                aria-label="Toggle base model"
                style={ICON_BTN_STYLE}
              >
                <EyeIcon style={ICON_14} />
              </button>

              <button
                type="button"
                className={`tool-btn${canvasLayers.nodeLabels ? " active" : ""}`}
                onClick={() => setLayer("nodeLabels", !canvasLayers.nodeLabels)}
                data-tooltip="Toggle node labels"
                data-tooltip-pos="bottom"
                style={{ fontSize: 11, fontWeight: 600 }}
              >
                Aa
              </button>

              <button
                type="button"
                className={`tool-btn${canvasLayers.linkLabels ? " active" : ""}`}
                onClick={() => setLayer("linkLabels", !canvasLayers.linkLabels)}
                data-tooltip="Toggle link labels"
                data-tooltip-pos="bottom"
                style={{ fontSize: 11, fontWeight: 600 }}
              >
                Ll
              </button>
            </div>
          </div>

          {/* Annotation summary (measure) */}
          {(activeTool === "measure" ||
            measureGeoPts.length > 0 ||
            measurePts.length > 0) && (
            <AnnotationSummary
              tool={activeTool}
              measurePts={measurePts}
              measureGeoPts={measureGeoPts}
              measureDistanceM={measureDistanceM}
              viewMode={viewMode}
              onClear={clearAnnotations}
            />
          )}

          {/* Floating viewport controls */}
          <div
            className="canvas-toolbar"
            style={{
              position: "absolute",
              right: 12,
              bottom: 12,
              zIndex: 11,
              flexDirection: "column",
              gap: 8,
            }}
          >
            <div style={{ display: "flex", flexDirection: "column", gap: 0 }}>
              <button
                type="button"
                className="tool-btn"
                onClick={() => setZoomInKey((k) => k + 1)}
                data-tooltip="Zoom in"
                data-tooltip-pos="left"
                aria-label="Zoom in"
                style={{
                  borderBottomLeftRadius: 0,
                  borderBottomRightRadius: 0,
                }}
              >
                <PlusIcon style={ICON_14} />
              </button>

              <button
                type="button"
                className="tool-btn"
                onClick={() => setZoomOutKey((k) => k + 1)}
                data-tooltip="Zoom out"
                data-tooltip-pos="left"
                aria-label="Zoom out"
                style={{
                  borderTopLeftRadius: 0,
                  borderTopRightRadius: 0,
                  marginTop: -1,
                }}
              >
                <MinusIcon style={ICON_14} />
              </button>
            </div>

            <button
              type="button"
              className="tool-btn"
              onClick={() => setResetNorthKey((k) => k + 1)}
              disabled={mapOnly}
              data-tooltip={mapOnlyTooltip("Reset north")}
              data-tooltip-pos="left"
              aria-label="Reset north"
              style={mapOnlyDim}
            >
              <ArrowsRightLeftIcon style={ICON_14} />
            </button>

            <button
              type="button"
              className="tool-btn"
              onClick={() => setMapFitKey((k) => k + 1)}
              data-tooltip="Fit network"
              data-tooltip-pos="left"
              aria-label="Fit network"
            >
              <ArrowsPointingOutIcon style={ICON_14} />
            </button>
          </div>

          {/* Inspector panel — node or link detail view */}
          {inspectorView === "node" && stableSelectedNode && (
            <NodeInspector
              node={stableSelectedNode}
              onClose={clearSelection}
              onOpenInEditor={() => {
                setProjectView("editor");
              }}
              onZoomTo={() =>
                setFlyToState((s) => ({
                  nodeId: selectedNodeId,
                  linkId: null,
                  key: s.key + 1,
                }))
              }
              disableZoomTo={!selectedNodeHasCoordinates}
              onDelete={() =>
                setPendingDelete({
                  kind: stableSelectedNode.type,
                  id: stableSelectedNode.id,
                })
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
            />
          )}
          {inspectorView === "link" && stableSelectedLink && (
            <LinkInspector
              link={stableSelectedLink}
              onClose={clearSelection}
              onOpenInEditor={() => {
                setProjectView("editor");
              }}
              onZoomTo={() =>
                setFlyToState((s) => ({
                  nodeId: null,
                  linkId: selectedLinkId,
                  key: s.key + 1,
                }))
              }
              disableZoomTo={!selectedLinkHasCoordinates}
              onDelete={() =>
                setPendingDelete({
                  kind: stableSelectedLink.type,
                  id: stableSelectedLink.id,
                })
              }
              onLocateNode={(id) => {
                if (nodeMap.has(id)) selectNode(id);
              }}
              linkVar={linkVar}
              ranges={stableResultMeta?.ranges}
              hasSimulation={!!stableResultMeta}
              isTransitioning={!!stableResultMeta && !linkIsEnriched}
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
          <span style={{ color: "var(--text-tertiary)", fontSize: 12 }}>
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
