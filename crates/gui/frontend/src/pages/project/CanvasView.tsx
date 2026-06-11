import {
  ArrowsRightLeftIcon,
  ChevronUpDownIcon,
  CursorArrowRaysIcon,
  EyeIcon,
  LinkIcon,
  MapPinIcon,
  PencilSquareIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useActiveProject, useAppState, useSimulation } from "../../AppContext";
import { AnnotationSummary, MeasureOverlay } from "../../canvas/Annotations";
import type { BasemapStyle } from "../../canvas/Basemap";
import {
  COMMON_CRS,
  haversineMeters,
  reprojectNodes,
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
  updateProjectCrs,
  useLinks,
  useNodes,
  useSimParams,
} from "../../hooks";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";
import { CanvasErrorBoundary } from "./CanvasView/CanvasErrorBoundary";
import { CoordStatusIndicator } from "./CanvasView/CoordStatusIndicator";

export function CanvasView() {
  const { activeScenarioId, setProjectView, projectView } = useAppState();
  const { project } = useActiveProject();
  const { bumpNetwork, markEdited } = useNetworkVersion();
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

  // ── Map fit key ──────────────────────────────────────────────────────
  // Increments only on project switch so MapCanvas resets its view to fit
  // the new network.  Does NOT increment on scenario switch so the user's
  // chosen pan/zoom position is preserved during scenario comparisons.
  const [mapFitKey, setMapFitKey] = useState(0);
  useEffect(() => {
    setMapFitKey((k) => k + 1);
  }, []);

  // ── Measure clicks ──────────────────────────────────────────
  // In map mode: geo points { lng, lat }. In schematic mode: SVG ClickPoints.
  const [measureGeoPts, setMeasureGeoPts] = useState<
    { lng: number; lat: number }[]
  >([]);
  const [measurePts, setMeasurePts] = useState<ClickPoint[]>([]);
  const svgRef = useRef<SVGSVGElement | null>(null);

  // Stable refs for keyboard handler so it never goes stale on selection changes.
  const selectedNodeIdRef = useRef<string | null>(null);
  const selectedLinkIdRef = useRef<string | null>(null);
  const nodeMapRef = useRef<Map<string, (typeof allNodes)[number]>>(new Map());
  const linkMapRef = useRef<Map<string, (typeof allLinks)[number]>>(new Map());

  // ── Simulation state ─────────────────────────────────────────────
  const { resultMeta, resultMetaLoading } = useSimulation();
  // `stableResultMeta` lags behind `resultMeta` while metadata is loading.
  // Once loading settles, it mirrors the active scenario exactly (including
  // null for unsimulated scenarios) so overlays cannot bleed across switches.
  const [stableResultMeta, setStableResultMeta] =
    useState<typeof resultMeta>(null);
  // Reset to null when the project changes (different network, stale ranges invalid).
  useEffect(() => {
    setStableResultMeta(null);
  }, []);
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
  useEffect(() => {
    setCurrentPeriodResult(null);
  }, []);

  useEffect(() => {
    if (!resultMeta || !project?.id) {
      // No simulation exists for this scenario — discard any stale period result
      // so the canvas and inspector show the "no results" state.
      setCurrentPeriodResult(null);
      return;
    }
    let cancelled = false;
    getPeriodResults(project.id, currentHour, activeScenarioId).then((r) => {
      if (!cancelled) {
        setCurrentPeriodResult(r);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [project?.id, currentHour, resultMeta, activeScenarioId]);

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
  const [hoverHour, setHoverHour] = useState<number | null>(null);

  // `maxStep` is the last valid step index: 0..maxStep.
  // Derived from stableResultMeta when available (covers multi-period results),
  // with a fallback for when no simulation has run yet.
  const maxStep = stableResultMeta ? stableResultMeta.times.length - 1 : 24;
  const isSteadyState = simParams != null && simParams.duration <= 0;

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
  // Cmd+Z / Ctrl+Z = undo; Cmd+Shift+Z / Ctrl+Y = redo.
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
          setMeasurePts([]);
          setMeasureGeoPts([]);
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
        case "`":
          setViewMode((v) => (v === "schematic" ? "map" : "schematic"));
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
  }, [maxStep, projectView]);

  const baseNodes = useNodes();
  const baseLinks = useLinks();
  // ── CRS reprojection ────────────────────────────────────────────────────
  // EPANET [COORDINATES] carry no CRS tag. We default to WGS84 (pass-through).
  // When the user sets a different source CRS we reproject the raw x/y to
  // WGS84 before passing them to MapCanvas. Schematic mode uses the BFS layout
  // so it is never affected by CRS — only geo/map mode needs this.
  const [showCrsDropdown, setShowCrsDropdown] = useState(false);
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
    // Only re-sync when the project identity itself changes, not on every render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [project?.sourceCrs]);

  // Raw positional nodes (no pressure/demand merged yet) used for CRS sniffing
  // and reprojection. Stable across timeline scrubs.
  const rawPositionNodes = baseNodes;

  // Auto-detect CRS when the network identity changes (new load or project switch).
  // Warns the user if coordinates look projected so they know to pick a CRS,
  // but never forces a view-mode switch — the user stays in control.
  useEffect(() => {
    if (rawPositionNodes.length === 0) return;
    // Reset dismissible warnings and any prior reprojection error on new network.
    setCrsError(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
            "Coordinates are outside WGS84 range — set the source CRS in the toolbar.",
        } as { nodes: typeof rawPositionNodes; error: string | null };
      }
      return { nodes: rawPositionNodes, error: null as string | null };
    }
    try {
      return {
        nodes: reprojectNodes(rawPositionNodes, sourceCrs),
        error: null,
      };
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      return { nodes: rawPositionNodes, error: msg };
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sourceCrs, rawPositionNodes.some, rawPositionNodes]);

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
    for (const n of reprojectedPositionNodes) m.set(n.id, { x: n.x, y: n.y });
    return m;
  }, [reprojectedPositionNodes]);
  const allNodes = useMemo(() => {
    const base = baseNodes;
    // Merge reprojected x/y positions into each node.  This is done regardless
    // of overlayMode so MapCanvas always gets WGS84 coords in map mode.
    const withPos = base.map((n) => {
      const pos = reprojectedXY.get(n.id);
      return pos ? { ...n, x: pos.x, y: pos.y } : n;
    });
    // Flat-array period result from getPeriodResults (index = network order).
    if (
      currentPeriodResult &&
      currentPeriodResult.nodePressure.length === withPos.length
    ) {
      return withPos.map((n, i) => ({
        ...n,
        pressure: currentPeriodResult.nodePressure[i],
        demand: currentPeriodResult.nodeDemand[i],
        head: currentPeriodResult.nodeHead[i],
        quality: currentPeriodResult.nodeQuality?.[i] ?? null,
      }));
    }
    return withPos;
  }, [baseNodes, reprojectedXY, currentPeriodResult]);

  const allLinks = useMemo(() => {
    const base = baseLinks;
    // Flat-array period result.
    if (
      currentPeriodResult &&
      currentPeriodResult.linkFlow.length === base.length
    ) {
      return base.map((l, i) => ({
        ...l,
        velocity: currentPeriodResult.linkVelocity[i],
        flow: currentPeriodResult.linkFlow[i],
        status: currentPeriodResult.linkStatus[i],
        quality: currentPeriodResult.linkQuality?.[i] ?? null,
      }));
    }
    return base;
  }, [baseLinks, currentPeriodResult]);

  // Keep the selection context's sim data in sync so the rail can display
  // live result values without re-fetching from the backend.
  useEffect(() => {
    // Only push to the rail when allNodes is actually enriched with sim data,
    // or when there is no sim data to expect. This prevents the network list
    // from flashing blank during the window between bumpNetwork() updating
    // baseNodes (null pressure) and getPeriodResults() delivering new results.
    const enriched =
      currentPeriodResult == null || allNodes.some((n) => n.pressure != null);
    if (enriched) setSimData(allNodes, allLinks);
  }, [allNodes, allLinks, setSimData, currentPeriodResult]);

  // For MapCanvas: hold the last enriched arrays so the canvas stays colored
  // during the bumpNetwork → getPeriodResults window.  Same latch pattern as
  // stableResultMeta — only update when the data actually carries sim values,
  // so the deck.gl layers never render a frame with null-pressure grey nodes.
  // Nodes and links are always enriched together (same getPeriodResults call),
  // so a single enrichment flag covers both arrays.
  const stableCanvasNodesRef = useRef<typeof allNodes>(allNodes);
  const stableCanvasLinksRef = useRef<typeof allLinks>(allLinks);
  // The stable-latch prevents flicker during the brief window between a
  // bumpNetwork() call (which delivers new baseNodes without pressure data)
  // and getPeriodResults() resolving with sim values.  It must NOT apply
  // when the network topology changed (node/link added or deleted) because
  // that would keep the deleted element visible on the canvas indefinitely.
  const topologyChanged =
    allNodes.length !== stableCanvasNodesRef.current.length ||
    allLinks.length !== stableCanvasLinksRef.current.length;
  const canvasEnriched =
    !stableResultMeta ||
    allNodes.some((n) => n.pressure != null) ||
    topologyChanged;
  if (canvasEnriched) stableCanvasNodesRef.current = allNodes;
  if (canvasEnriched) stableCanvasLinksRef.current = allLinks;
  const canvasNodes = canvasEnriched ? allNodes : stableCanvasNodesRef.current;
  const canvasLinks = canvasEnriched ? allLinks : stableCanvasLinksRef.current;

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
      (activeTool === "add-node" ||
        activeTool === "add-link" ||
        activeTool === "measure")
    ) {
      setActiveTool("select");
    }
  }, [viewMode, activeTool]); // intentionally omit activeTool — only re-evaluate on mode switch

  const svgCursor = activeTool === "measure" ? "crosshair" : "default";

  const handleNodeMoved = useCallback(
    async (id: string, x: number, y: number) => {
      if (!project) return;
      await patchNodePosition(id, x, y);
      await saveProjectOnDisk(project.id, activeScenarioId);
      markEdited(activeScenarioId);
      bumpNetwork();
    },
    [project, activeScenarioId, markEdited, bumpNetwork],
  );

  const handleConfirmDelete = useCallback(async () => {
    if (!pendingDelete || !project) return;
    const { kind, id } = pendingDelete;
    setPendingDelete(null);
    clearSelection();
    await deleteElement(kind, id);
    await saveProjectOnDisk(project.id, activeScenarioId);
    markEdited(activeScenarioId);
    bumpNetwork();
  }, [
    pendingDelete,
    project,
    activeScenarioId,
    markEdited,
    bumpNetwork,
    clearSelection,
  ]);

  // ── Node / link ID suggestion ─────────────────────────────────────────────
  // Generates a short unique ID by finding the first gap in the existing IDs.
  // Accepts the node kind ("junction" | "reservoir" | "tank") and picks the
  // appropriate prefix automatically.
  const NODE_KIND_PREFIX: Record<string, string> = {
    junction: "J",
    reservoir: "R",
    tank: "T",
  };
  const suggestNodeId = useCallback(
    (kind: string) => {
      const prefix = NODE_KIND_PREFIX[kind] ?? "N";
      const existing = new Set(allNodes.map((n) => n.id));
      for (let i = 1; i <= 9999; i++) {
        const id = `${prefix}${i}`;
        if (!existing.has(id)) return id;
      }
      return `${prefix}${Date.now()}`;
      // eslint-disable-next-line react-hooks/exhaustive-deps
    },
    [allNodes, NODE_KIND_PREFIX],
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
      bumpNetwork();
    },
    [pendingCreateNode, project, activeScenarioId, markEdited, bumpNetwork],
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
      bumpNetwork();
    },
    [pendingCreateLink, project, activeScenarioId, markEdited, bumpNetwork],
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
  // profile points stay anchored to the network even when the SVG is scaled.
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
      setShowCrsDropdown(false);
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
  // anywhere outside the toolbar.  Triggers and panels are tagged with
  // `data-toolbar-dropdown`, so we can distinguish a click on the dropdown UI
  // (do nothing) from a click anywhere else (close all).
  useEffect(() => {
    if (!showBasemapDropdown && !showCrsDropdown) return;
    function onDown(e: PointerEvent) {
      const target = e.target as HTMLElement | null;
      if (target?.closest("[data-toolbar-dropdown]")) return;
      setShowBasemapDropdown(false);
      setShowCrsDropdown(false);
    }
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  }, [showBasemapDropdown, showCrsDropdown]);

  function clearAnnotations() {
    setMeasurePts([]);
    setMeasureGeoPts([]);
  }

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
              viewMode={viewMode}
              nodeVar={nodeVar}
              linkVar={linkVar}
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
            />
          </CanvasErrorBoundary>

          {/* Legend — visible only when simulation results exist */}
          {!!stableResultMeta && (
            <Legend
              nodeVar={nodeVar}
              setNodeVar={setNodeVar}
              linkVar={linkVar}
              setLinkVar={setLinkVar}
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
                  Map view requires valid WGS84 coordinates. Select the correct
                  CRS from the toolbar to reproject the network, or switch to
                  Schematic view.
                </span>
              </div>
            </div>
          )}

          {/* Legacy SVG annotation overlays (schematic mode only).
               pointer-events: none — deck.gl handles all interaction. */}
          {viewMode === "schematic" && (
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
                        ? "Geographic layout (M)"
                        : "Idealised orthogonal layout (S)"
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
                style={{
                  position: "relative",
                  opacity: viewMode !== "map" ? 0.38 : undefined,
                }}
              >
                <button
                  className="tool-btn"
                  disabled={viewMode !== "map"}
                  style={{
                    width: "auto",
                    padding: "0 8px",
                    fontSize: 12,
                    gap: 4,
                    display: "flex",
                    alignItems: "center",
                    cursor: viewMode !== "map" ? "not-allowed" : undefined,
                  }}
                  onClick={(e) => {
                    if (viewMode !== "map") return;
                    e.stopPropagation();
                    setShowBasemapDropdown((v) => !v);
                    setShowCrsDropdown(false);
                  }}
                  data-tooltip={
                    viewMode !== "map" ? "Map mode only" : "Basemap"
                  }
                  data-tooltip-pos="bottom"
                >
                  {basemap === "none"
                    ? "No basemap"
                    : basemap.charAt(0).toUpperCase() + basemap.slice(1)}{" "}
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
                      [
                        "streets",
                        "light",
                        "dark",
                        "none",
                      ] as BasemapStyle[]
                    ).map((b) => (
                      <button
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
                        {b === "none"
                          ? "No basemap"
                          : b.charAt(0).toUpperCase() + b.slice(1)}
                      </button>
                    ))}
                  </div>
                )}
              </div>

              {/* CRS picker */}
              <div
                data-toolbar-dropdown
                style={{
                  position: "relative",
                  opacity: viewMode !== "map" ? 0.38 : undefined,
                }}
              >
                <button
                  className="tool-btn"
                  disabled={viewMode !== "map"}
                  style={{
                    width: "auto",
                    padding: "0 8px",
                    fontSize: 12,
                    gap: 4,
                    display: "flex",
                    alignItems: "center",
                    cursor: viewMode !== "map" ? "not-allowed" : undefined,
                    color:
                      viewMode === "map" &&
                      sourceCrs !== "EPSG:4326" &&
                      !crsError
                        ? "var(--accent)"
                        : undefined,
                    borderColor:
                      viewMode === "map" && crsError
                        ? "var(--status-error)"
                        : undefined,
                  }}
                  onClick={(e) => {
                    if (viewMode !== "map") return;
                    e.stopPropagation();
                    setShowCrsDropdown((v) => !v);
                    setShowBasemapDropdown(false);
                  }}
                  data-tooltip={
                    viewMode !== "map"
                      ? "Map mode only"
                      : (crsError ?? "Source coordinate reference system")
                  }
                  data-tooltip-pos="bottom"
                >
                  {sourceCrs === "EPSG:4326" ? "CRS" : sourceCrs}{" "}
                  <ChevronUpDownIcon
                    style={{ width: 12, height: 12, verticalAlign: "middle" }}
                  />
                </button>
                {showCrsDropdown && viewMode === "map" && (
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
                      minWidth: 280,
                      zIndex: 20,
                    }}
                  >
                    <div
                      style={{
                        padding: "6px 10px 4px",
                        fontSize: 10,
                        color: "var(--text-tertiary)",
                        fontWeight: 600,
                        letterSpacing: "0.05em",
                        textTransform: "uppercase",
                      }}
                    >
                      Source CRS
                    </div>
                    {COMMON_CRS.map((c) => (
                      <button
                        key={c.epsg}
                        onClick={() => {
                          setSourceCrs(c.epsg);
                          if (project?.id) updateProjectCrs(project.id, c.epsg);
                          setShowCrsDropdown(false);
                        }}
                        style={{
                          display: "block",
                          width: "100%",
                          padding: "7px 12px",
                          border: "none",
                          background:
                            sourceCrs === c.epsg
                              ? "var(--accent-dim)"
                              : "transparent",
                          color:
                            sourceCrs === c.epsg
                              ? "var(--accent)"
                              : "var(--text-secondary)",
                          cursor: "pointer",
                          fontSize: 12,
                          textAlign: "left",
                          fontFamily: "var(--font-ui)",
                        }}
                      >
                        {c.label}
                      </button>
                    ))}
                    {/* Custom EPSG entry */}
                    <div
                      style={{
                        padding: "6px 10px 8px",
                        borderTop: "1px solid var(--border)",
                      }}
                    >
                      <div
                        style={{
                          fontSize: 10,
                          color: "var(--text-tertiary)",
                          marginBottom: 4,
                        }}
                      >
                        Custom EPSG code
                      </div>
                      <form
                        onSubmit={(e) => {
                          e.preventDefault();
                          const val = (
                            e.currentTarget.elements.namedItem(
                              "epsg",
                            ) as HTMLInputElement
                          ).value
                            .trim()
                            .toUpperCase();
                          const code = val.startsWith("EPSG:")
                            ? val
                            : `EPSG:${val}`;
                          setSourceCrs(code);
                          if (project?.id) updateProjectCrs(project.id, code);
                          setShowCrsDropdown(false);
                        }}
                        style={{ display: "flex", gap: 6 }}
                      >
                        <input
                          name="epsg"
                          placeholder="e.g. 28355"
                          style={{
                            flex: 1,
                            fontSize: 12,
                            padding: "3px 7px",
                            background: "var(--bg-input, var(--bg-card))",
                            border: "1px solid var(--border)",
                            borderRadius: 4,
                            color: "var(--text-primary)",
                            fontFamily: "var(--font-mono)",
                          }}
                        />
                        <button
                          type="submit"
                          className="tool-btn"
                          style={{ padding: "0 8px", fontSize: 11 }}
                        >
                          Apply
                        </button>
                      </form>
                      {crsError && (
                        <div
                          style={{
                            fontSize: 11,
                            color: "var(--status-error)",
                            marginTop: 4,
                          }}
                        >
                          {crsError}
                        </div>
                      )}
                    </div>
                  </div>
                )}
              </div>

              <div className="tool-divider" />

              {/* ── BOTH MODES ───────────────────────────────────────────────── */}

              <button
                className={`tool-btn${activeTool === "select" ? " active" : ""}`}
                onClick={() => setActiveTool("select")}
                data-tooltip="Select (S)"
                data-tooltip-pos="bottom"
                aria-label="Select"
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                }}
              >
                <CursorArrowRaysIcon style={{ width: 14, height: 14 }} />
              </button>

              <button
                className={`tool-btn${activeTool === "edit" ? " active" : ""}`}
                onClick={() => setActiveTool("edit")}
                data-tooltip="Edit / move nodes (E)"
                data-tooltip-pos="bottom"
                aria-label="Edit"
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                }}
              >
                <PencilSquareIcon style={{ width: 14, height: 14 }} />
              </button>

              <button
                className={`tool-btn${activeTool === "add-node" ? " active" : ""}`}
                disabled={viewMode !== "map"}
                onClick={() => setActiveTool("add-node")}
                data-tooltip={
                  viewMode !== "map" ? "Map mode only" : "Add node (N)"
                }
                data-tooltip-pos="bottom"
                aria-label="Add node"
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  opacity: viewMode !== "map" ? 0.38 : undefined,
                  cursor: viewMode !== "map" ? "not-allowed" : undefined,
                }}
              >
                <MapPinIcon style={{ width: 14, height: 14 }} />
              </button>

              <button
                className={`tool-btn${activeTool === "add-link" ? " active" : ""}`}
                disabled={viewMode !== "map"}
                onClick={() => setActiveTool("add-link")}
                data-tooltip={
                  viewMode !== "map" ? "Map mode only" : "Add link (L)"
                }
                data-tooltip-pos="bottom"
                aria-label="Add link"
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  opacity: viewMode !== "map" ? 0.38 : undefined,
                  cursor: viewMode !== "map" ? "not-allowed" : undefined,
                }}
              >
                <LinkIcon style={{ width: 14, height: 14 }} />
              </button>

              {/* Measure distance */}
              <button
                className={`tool-btn${activeTool === "measure" ? " active" : ""}`}
                disabled={viewMode !== "map"}
                onClick={() => {
                  setActiveTool("measure");
                  setMeasurePts([]);
                  setMeasureGeoPts([]);
                }}
                data-tooltip={
                  viewMode !== "map" ? "Map mode only" : "Measure distance (D)"
                }
                data-tooltip-pos="bottom"
                aria-label="Measure distance"
                style={{
                  fontSize: 12,
                  fontWeight: 600,
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                  opacity: viewMode !== "map" ? 0.38 : undefined,
                  cursor: viewMode !== "map" ? "not-allowed" : undefined,
                }}
              >
                <ArrowsRightLeftIcon style={{ width: 14, height: 14 }} />
              </button>

              {(measureGeoPts.length > 0 || measurePts.length > 0) &&
                viewMode === "map" && (
                  <button
                    className="tool-btn"
                    onClick={clearAnnotations}
                    data-tooltip="Clear annotations"
                    data-tooltip-pos="bottom"
                    aria-label="Clear annotations"
                    style={{
                      fontSize: 11,
                      color: "var(--text-tertiary)",
                      display: "inline-flex",
                      alignItems: "center",
                      justifyContent: "center",
                    }}
                  >
                    <XMarkIcon style={{ width: 14, height: 14 }} />
                  </button>
                )}

              <div className="tool-divider" />

              {/* Layer visibility toggles */}
              <button
                className={`tool-btn${canvasLayers.model ? " active" : ""}`}
                onClick={() => setLayer("model", !canvasLayers.model)}
                data-tooltip="Toggle base model"
                data-tooltip-pos="bottom"
                aria-label="Toggle base model"
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  justifyContent: "center",
                }}
              >
                <EyeIcon style={{ width: 14, height: 14 }} />
              </button>

              <button
                className={`tool-btn${canvasLayers.nodeLabels ? " active" : ""}`}
                onClick={() => setLayer("nodeLabels", !canvasLayers.nodeLabels)}
                data-tooltip="Toggle node labels"
                data-tooltip-pos="bottom"
                style={{ fontSize: 11, fontWeight: 600 }}
              >
                Aa
              </button>

              <button
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
          hoverHour={hoverHour}
          setHoverHour={setHoverHour}
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
