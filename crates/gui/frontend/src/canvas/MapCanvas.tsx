import type {
  Layer,
  OrthographicViewState,
  ViewStateChangeParameters,
} from "@deck.gl/core";
import { COORDINATE_SYSTEM, Deck, OrthographicView } from "@deck.gl/core";
import {
  LineLayer,
  PathLayer,
  ScatterplotLayer,
  TextLayer,
} from "@deck.gl/layers";
import { MapboxOverlay } from "@deck.gl/mapbox";
import maplibregl from "maplibre-gl";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Link, Node, PeriodResults } from "../hooks";
import { startPerfSpan } from "../perfTrace";
import type { BasemapStyle } from "./Basemap";
import type { CompareDeltas } from "./compare";
import { FlowPathLayer } from "./FlowPathLayer";
import { useCanvasLayers } from "./layers-context";
import {
  divergingRgba,
  hashStr,
  linkRgba,
  nodeRgba,
  type RGBA,
} from "./MapCanvas/colorUtils";
import {
  fitMapExtents,
  geoBounds,
  orthoCenterFromMap,
  roughGeoViewState,
} from "./MapCanvas/geoUtils";
import { computeSchematicLayout } from "./schematicLayout";
import type { CanvasTool, LinkVariable, NodeVariable, ViewMode } from "./types";

// Blank MapLibre style used when the user selects "No basemap". Renders a
// solid background with no tile sources so no network requests are made.
const BLANK_STYLE: maplibregl.StyleSpecification = {
  version: 8,
  sources: {},
  layers: [
    {
      id: "background",
      type: "background",
      paint: { "background-color": "#16181c" },
    },
  ],
};

// "streets" = OpenFreeMap Liberty (full coloured streets)
// "light"   = OpenFreeMap Positron (minimal light theme)
// "dark"    = OpenFreeMap Dark (dark theme)
// "none"    = tile-free blank background
const MAP_STYLES: Record<BasemapStyle, string | maplibregl.StyleSpecification> =
  {
    streets: "https://tiles.openfreemap.org/styles/liberty",
    light: "https://tiles.openfreemap.org/styles/positron",
    dark: "https://tiles.openfreemap.org/styles/dark",
    none: BLANK_STYLE,
  };

const EMPTY_SCHEMATIC_COORDS: Map<string, [number, number]> = new Map();

// Glow/halo ring tables (outer → inner) for the hover/selection highlight
// layers built in buildLayers. Alphas/widths/radius pads are visual tuning —
// the layer ids derived from these suffixes must stay stable (deck.gl matches
// layers by id).
const LINK_HOVER_GLOW = [
  { suffix: "outer", alpha: 20, width: 18 },
  { suffix: "mid", alpha: 50, width: 9 },
  { suffix: "inner", alpha: 90, width: 4 },
];
const LINK_SELECTION_GLOW = [
  { suffix: "outer", alpha: 40, width: 22 },
  { suffix: "mid", alpha: 90, width: 10 },
  { suffix: "inner", alpha: 170, width: 5 },
];
const NODE_HOVER_GLOW = [
  { suffix: "outer", alpha: 18, radiusPad: 14 },
  { suffix: "mid", alpha: 40, radiusPad: 8 },
  { suffix: "inner", alpha: 70, radiusPad: 4 },
];
const NODE_SELECTION_GLOW = [
  { suffix: "outer", alpha: 35, radiusPad: 18 },
  { suffix: "mid", alpha: 80, radiusPad: 11 },
  { suffix: "inner", alpha: 140, radiusPad: 5 },
];

/** Above this many on-screen labels, label layers render nothing — the text
 * would be unreadable overlap anyway and TextLayer tesselation at 46k ids
 * freezes the frame. Zoom in (or filter) to see labels on huge networks. */
const MAX_LABELS = 1500;

type GeoViewState = ReturnType<typeof roughGeoViewState>;
type SchematicViewState = ReturnType<typeof orthoCenterFromMap>;
type CanvasViewState = GeoViewState | SchematicViewState;

interface MapCanvasProps {
  nodes: Node[];
  links: Link[];
  viewMode: ViewMode;
  nodeVar: NodeVariable;
  linkVar: LinkVariable;
  /** Animate the Flow/Velocity pulse effect. Already accounts for the user
   * toggle and the "Reduce motion" accessibility setting. */
  animateLinks?: boolean;
  /** Flat per-period result arrays (network order). Passed separately from
   * nodes/links so a timeline scrub changes only this prop — the node/link
   * arrays keep their identity and deck.gl only re-evaluates colours. */
  periodResult?: PeriodResults | null;
  /** Scenario-comparison Δ overlay (active − baseline). When set, node/link
   * colours come from the delta arrays through the diverging ramp instead of
   * the absolute-value ramps. Identity-stable (memoized in CanvasView) — it
   * participates in updateTriggers, so it must only change when the deltas
   * actually change. `null`/absent = normal (non-compare) rendering. */
  compare?: CompareDeltas | null;
  basemap: BasemapStyle;
  selectedNodeId: string | null;
  onSelectNode: (id: string | null) => void;
  selectedLinkId: string | null;
  onSelectLink: (id: string | null) => void;
  /** Result ranges used to normalise colour scales. */
  headMin?: number;
  headMax?: number;
  demandMin?: number;
  demandMax?: number;
  flowMax?: number;
  qualityMin?: number;
  qualityMax?: number;
  /** "relative" = full min–max ramp (default); "threshold" = user-defined bands. */
  colorMode?: "relative" | "threshold";
  /** Custom pressure thresholds (low / required / high in metres). */
  pressureThresholds?: { low: number; required: number; high: number };
  /** Custom velocity thresholds used when colorMode is "threshold". */
  velocityThresholds?: { low: number; target: number; high: number };
  /** Custom flow-magnitude thresholds used when colorMode is "threshold". */
  flowThresholds?: { low: number; target: number; high: number };
  /** Active canvas tool; affects cursor and interaction mode. */
  tool?: CanvasTool;
  /** Called (after mouseup) when the user drags a node to a new position.
   * `x` and `y` are geographic coordinates (longitude and latitude). */
  onNodeMoved?: (id: string, x: number, y: number) => void;
  /** Called when the user clicks a point in measure mode. */
  onMeasurePoint?: (lng: number, lat: number) => void;
  /** Called when the user clicks empty canvas in add-node mode. */
  onCreateNodeRequest?: (lng: number, lat: number) => void;
  /** Called when the user selects two nodes in add-link mode. */
  onCreateLinkRequest?: (fromId: string, toId: string) => void;
  /** When flyToKey changes and flyToNodeId/flyToLinkId is set, the canvas animates to that element. */
  flyToNodeId?: string | null;
  flyToLinkId?: string | null;
  flyToKey?: number;
  /** Increment to force the map/schematic to fit the full network extent.
   * Should change only on project switch (not scenario switch) so the user's
   * view position is preserved across scenario comparisons. */
  fitKey?: number;
  /** Increment to zoom in one step in the active view. */
  zoomInKey?: number;
  /** Increment to zoom out one step in the active view. */
  zoomOutKey?: number;
  /** Increment to reset map bearing/pitch (north up). Map mode only. */
  resetNorthKey?: number;
  /** Whether canvas is the currently active project tab. */
  isActive?: boolean;
}

// Memoized: CanvasView re-renders on many interactions that don't affect the
// canvas (toasts, tool state, timeline hover); with ~46k-element data arrays a
// wasted re-execution here is expensive. All props are primitives, stable
// useCallback handlers, or memoized arrays, so shallow comparison is safe.
export const MapCanvas = memo(function MapCanvas({
  nodes,
  links,
  viewMode,
  nodeVar,
  linkVar,
  animateLinks = true,
  periodResult = null,
  compare = null,
  basemap,
  selectedNodeId,
  onSelectNode,
  selectedLinkId,
  onSelectLink,
  headMin = 0,
  headMax = 100,
  demandMin = 0,
  demandMax = 1,
  flowMax = 1,
  qualityMin = 0,
  qualityMax = 1,
  colorMode = "relative" as const,
  pressureThresholds,
  velocityThresholds,
  flowThresholds,
  tool = "select",
  onNodeMoved,
  onCreateNodeRequest,
  onCreateLinkRequest,
  onMeasurePoint,
  flyToNodeId,
  flyToLinkId,
  flyToKey,
  fitKey,
  zoomInKey,
  zoomOutKey,
  resetNorthKey,
  isActive = true,
}: MapCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapElRef = useRef<HTMLDivElement>(null);
  const deckHostRef = useRef<HTMLDivElement>(null);
  const { layers: canvasLayers } = useCanvasLayers();
  const [hoveredNodeId, setHoveredNodeId] = useState<string | null>(null);
  const [hoveredLinkId, setHoveredLinkId] = useState<string | null>(null);
  const hoveredNodeIdRef = useRef<string | null>(null);
  const hoveredLinkIdRef = useRef<string | null>(null);
  const selectedNodeIdRef = useRef<string | null>(selectedNodeId);
  const selectedLinkIdRef = useRef<string | null>(selectedLinkId);
  const onSelectNodeRef = useRef(onSelectNode);
  const mapRef = useRef<maplibregl.Map | null>(null);
  const overlayRef = useRef<MapboxOverlay | null>(null);
  const deckRef = useRef<Deck<OrthographicView> | null>(null);
  const deckCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const draggingNodePosRef = useRef<{
    id: string;
    lng: number;
    lat: number;
  } | null>(null);
  const ghostLinkRef = useRef<{
    from: [number, number];
    to: [number, number];
  } | null>(null);
  // Measure tool: point A anchor + live cursor position for rubber-band line.
  const measureAnchorRef = useRef<[number, number] | null>(null);
  const measureCursorRef = useRef<[number, number] | null>(null);
  // Seeded lazily on first render: roughGeoViewState scans every node, so it
  // must not run as a useRef initializer argument (those are evaluated on
  // every render even though only the first value is kept).
  const viewStateLazyRef = useRef<CanvasViewState | null>(null);
  if (viewStateLazyRef.current === null) {
    viewStateLazyRef.current = roughGeoViewState(nodes);
  }
  const viewStateRef = viewStateLazyRef as { current: CanvasViewState };
  const prevViewModeRef = useRef<ViewMode | null>(null);
  const prevBasemapRef = useRef<BasemapStyle>(basemap);
  const orthoViewRef = useRef(
    new OrthographicView({ id: "main", controller: true }),
  );
  const flowAnimRef = useRef(0);
  const buildLayersRef = useRef<() => Layer[]>(() => []);
  const firstFrameSpanRef = useRef<ReturnType<typeof startPerfSpan> | null>(
    null,
  );
  const firstFrameKeyRef = useRef<string>("");
  const lastFirstFrameTraceRef = useRef<{ key: string; ts: number }>({
    key: "",
    ts: -Infinity,
  });
  const firstFramePendingRef = useRef(false);
  const firstFrameRafRef = useRef<number | null>(null);
  const prevActivePerfRef = useRef(isActive);
  const prevFitKeyPerfRef = useRef(fitKey);
  const isActiveRef = useRef(isActive);

  // Refs kept current for stable closures used in drag/edit handlers.
  const toolRef = useRef<CanvasTool>(tool);
  const viewModeRef = useRef<ViewMode>(viewMode);
  const onNodeMovedRef = useRef(onNodeMoved);
  const onCreateNodeRequestRef = useRef(onCreateNodeRequest);
  const onMeasurePointRef = useRef(onMeasurePoint);
  const onCreateLinkRequestRef = useRef(onCreateLinkRequest);
  const nodesRef = useRef(nodes);
  const linksRef = useRef(links);
  // Initialised empty; kept current by useEffect once geoCoords is available.
  const geoCoordsRef = useRef<Map<string, [number, number]>>(new Map());
  const draggingNodeIdRef = useRef<string | null>(null);
  // Pending 5 s drag-override fallback timer (armed on drop, see map mouseup).
  const dragFallbackTimerRef = useRef<number | null>(null);
  // Tracks whether the current mousedown actually moved — suppresses deck.gl onClick.
  const didDragRef = useRef(false);
  // In add-link mode: the ID of the first selected node, waiting for the second.
  const pendingLinkFromIdRef = useRef<string | null>(null);

  // Lazy schematic layout: the BFS layout over 46k+ elements is only needed
  // in schematic mode, and many sessions never leave map mode. Cached by
  // nodes/links identity so switching back to schematic is instant.
  const schematicCacheRef = useRef<{
    nodes: Node[];
    links: Link[];
    coords: Map<string, [number, number]>;
  } | null>(null);
  const schematicCoords = useMemo(() => {
    const cache = schematicCacheRef.current;
    if (cache && cache.nodes === nodes && cache.links === links) {
      return cache.coords;
    }
    if (viewMode !== "schematic") {
      // Drop a stale cache rather than pinning an obsolete full generation
      // of nodes/links/coords in memory until schematic is next opened.
      schematicCacheRef.current = null;
      return EMPTY_SCHEMATIC_COORDS;
    }
    const coords = computeSchematicLayout(nodes, links);
    schematicCacheRef.current = { nodes, links, coords };
    return coords;
  }, [nodes, links, viewMode]);

  const markFirstFrame = useCallback((source: "map" | "schematic") => {
    if (!firstFramePendingRef.current) return;
    firstFramePendingRef.current = false;
    if (firstFrameRafRef.current != null) {
      cancelAnimationFrame(firstFrameRafRef.current);
    }
    firstFrameRafRef.current = requestAnimationFrame(() => {
      firstFrameRafRef.current = null;
      const now = performance.now();
      const key = `${firstFrameKeyRef.current}:${source}`;
      const duplicateRecent =
        lastFirstFrameTraceRef.current.key === key &&
        now - lastFirstFrameTraceRef.current.ts < 1200;
      if (!duplicateRecent) {
        firstFrameSpanRef.current?.end({ source });
        lastFirstFrameTraceRef.current = { key, ts: now };
      }
      firstFrameSpanRef.current = null;
      firstFrameKeyRef.current = "";
    });
  }, []);

  useEffect(() => {
    const becameActive = isActive && !prevActivePerfRef.current;
    const fitChanged = fitKey !== prevFitKeyPerfRef.current;
    prevActivePerfRef.current = isActive;
    prevFitKeyPerfRef.current = fitKey;

    if (!isActive) {
      firstFramePendingRef.current = false;
      if (firstFrameRafRef.current != null) {
        cancelAnimationFrame(firstFrameRafRef.current);
        firstFrameRafRef.current = null;
      }
      firstFrameSpanRef.current = null;
      return;
    }

    if (
      (becameActive || fitChanged) &&
      (nodes.length > 0 || links.length > 0)
    ) {
      if (firstFramePendingRef.current) return;
      firstFramePendingRef.current = true;
      firstFrameKeyRef.current = `${viewMode}:${nodes.length}:${links.length}`;
      firstFrameSpanRef.current = startPerfSpan("canvas-first-frame", {
        viewMode,
        nodeCount: nodes.length,
        linkCount: links.length,
        fitKey: fitKey ?? null,
      });
    }
  }, [fitKey, isActive, links.length, nodes.length, viewMode]);

  useEffect(() => {
    isActiveRef.current = isActive;
  }, [isActive]);

  useEffect(
    () => () => {
      if (firstFrameRafRef.current != null) {
        cancelAnimationFrame(firstFrameRafRef.current);
      }
    },
    [],
  );

  const geoCoords = useMemo(() => {
    const m = new Map<string, [number, number]>();
    for (const n of nodes) {
      if (n.x === 0 && n.y === 0) continue;
      m.set(n.id, [n.x, n.y]);
    }
    return m;
  }, [nodes]);

  useEffect(() => {
    selectedNodeIdRef.current = selectedNodeId;
  }, [selectedNodeId]);
  useEffect(() => {
    selectedLinkIdRef.current = selectedLinkId;
  }, [selectedLinkId]);
  useEffect(() => {
    onSelectNodeRef.current = onSelectNode;
  }, [onSelectNode]);
  useEffect(() => {
    toolRef.current = tool;
    // Picking is tool-gated; when a tool disables it, onHover(null) can never
    // fire, so clear any lingering hover state (stale glow + cursor).
    if (tool !== "select" && tool !== "edit") {
      hoveredLinkIdRef.current = null;
      setHoveredLinkId(null);
    }
    if (tool === "measure") {
      hoveredNodeIdRef.current = null;
      setHoveredNodeId(null);
    }
  }, [tool]);
  useEffect(() => {
    viewModeRef.current = viewMode;
  }, [viewMode]);
  useEffect(() => {
    onNodeMovedRef.current = onNodeMoved;
  }, [onNodeMoved]);
  useEffect(() => {
    onCreateNodeRequestRef.current = onCreateNodeRequest;
  }, [onCreateNodeRequest]);
  useEffect(() => {
    onMeasurePointRef.current = onMeasurePoint;
  }, [onMeasurePoint]);
  useEffect(() => {
    onCreateLinkRequestRef.current = onCreateLinkRequest;
  }, [onCreateLinkRequest]);
  useEffect(() => {
    nodesRef.current = nodes;
  }, [nodes]);
  useEffect(() => {
    linksRef.current = links;
  }, [links]);

  // When switching away from add-link mode, cancel any pending link and clear the ghost line.
  useEffect(() => {
    if (tool === "add-link") return;
    pendingLinkFromIdRef.current = null;
    const map = mapRef.current;
    if (!map) return;
    const ghostSrc = map.getSource("pending-link-source") as
      | maplibregl.GeoJSONSource
      | undefined;
    ghostSrc?.setData({ type: "FeatureCollection", features: [] });
    // Restore cursor.
    map.getCanvas().style.cursor = "";
  }, [tool]);

  // When switching away from measure mode, clear the anchor and cursor.
  useEffect(() => {
    if (tool === "measure") return;
    measureAnchorRef.current = null;
    measureCursorRef.current = null;
  }, [tool]);

  // Set crosshair cursor for placement tools.
  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;
    if (tool === "add-node" || tool === "add-link") {
      map.getCanvas().style.cursor = "crosshair";
    } else if (tool !== "edit") {
      map.getCanvas().style.cursor = "";
    }
  }, [tool]);
  useEffect(() => {
    geoCoordsRef.current = geoCoords;
  }, [geoCoords]);
  const schematicCoordsRef = useRef<Map<string, [number, number]>>(new Map());
  useEffect(() => {
    schematicCoordsRef.current = schematicCoords;
  }, [schematicCoords]);

  // Fly/zoom to a specific element when flyToKey changes.
  useEffect(() => {
    if (!isActive) return;
    if (flyToKey == null) return;
    const nodeId = flyToNodeId;
    const linkId = flyToLinkId;
    if (!nodeId && !linkId) return;

    if (viewMode === "map") {
      const map = mapRef.current;
      if (!map) return;
      if (nodeId) {
        const center = geoCoordsRef.current.get(nodeId);
        if (!center) return;
        // viewStateRef only tracks schematic view changes — MapLibre pans and
        // zooms never write it — so read the live zoom from the map itself.
        const mapZoom = map.getZoom();
        const currentZoom = Number.isFinite(mapZoom) ? mapZoom : 12;
        const zoom = Math.max(currentZoom, 14);
        map.flyTo({ center, zoom, curve: 1, duration: 800 });
      } else if (linkId) {
        const link = linksRef.current.find((l) => l.id === linkId);
        if (!link) return;
        const from = geoCoordsRef.current.get(link.fromId);
        const to = geoCoordsRef.current.get(link.toId);
        if (!from || !to) return;
        const bounds = new maplibregl.LngLatBounds(from, from).extend(to);
        map.fitBounds(bounds, { padding: 80, maxZoom: 18, duration: 800 });
      }
    } else {
      // Schematic mode — orthographic view
      const deck = deckRef.current;
      if (!deck) return;
      const coords = schematicCoordsRef.current;
      const { zoom: fitZoom } = orthoCenterFromMap(coords);
      if (nodeId) {
        const target = coords.get(nodeId);
        if (!target) return;
        // Use a bounded zoom relative to whole-network fit to avoid runaway
        // cumulative zooming in orthographic mode.
        const zoom = Math.min(fitZoom + 1, 10);
        const vs = {
          target: [target[0], target[1], 0] as [number, number, number],
          zoom,
        };
        viewStateRef.current = vs;
        deck.setProps({ viewState: vs });
      } else if (linkId) {
        const link = linksRef.current.find((l) => l.id === linkId);
        if (!link) return;
        const from = coords.get(link.fromId);
        const to = coords.get(link.toId);
        if (!from || !to) return;
        const cx = (from[0] + to[0]) / 2;
        const cy = (from[1] + to[1]) / 2;
        // Compute zoom so the link spans ~40% of the smaller viewport dimension.
        const canvas = deckCanvasRef.current;
        const viewW = canvas?.clientWidth ?? 800;
        const viewH = canvas?.clientHeight ?? 600;
        const linkUnits = Math.sqrt(
          (to[0] - from[0]) ** 2 + (to[1] - from[1]) ** 2,
        );
        const targetSpanPx = Math.min(viewW, viewH) * 0.4;
        // OrthographicView uses zoom in log2 scale (scale = 2^zoom). Convert
        // desired pixels-per-unit to zoom and cap relative to fit zoom.
        const zoom =
          linkUnits > 0
            ? Math.min(Math.log2(targetSpanPx / linkUnits), fitZoom + 3)
            : Math.min(fitZoom + 2, 10);
        const vs = { target: [cx, cy, 0] as [number, number, number], zoom };
        viewStateRef.current = vs;
        deck.setProps({ viewState: vs });
      }
    }
  }, [flyToKey, isActive, viewMode, flyToLinkId, flyToNodeId]);

  // ── deck.gl data arrays ────────────────────────────────────────────────────
  // Memoized so their identity is stable across renders that don't change the
  // network or coordinates. This matters at scale: the flow-animation RAF loop
  // and hover/selection state changes rebuild the *layers* every time, and
  // deck.gl decides whether to re-run accessors and re-upload attribute
  // buffers by comparing `data` identity. With ~46k nodes/links, rebuilding
  // these arrays per frame meant re-tesselating and re-uploading everything at
  // 60 fps; with stable identity those frames only update a uniform.
  const { linkData, nodeData, linkDatumById, nodeDatumById, anyLinkVertices } =
    useMemo(() => {
      const isSchematic = viewMode === "schematic";
      const coordMap = isSchematic ? schematicCoords : geoCoords;
      // Display path precomputed once per network/viewMode change (not per
      // accessor call over 46k links). Schematic mode ignores vertices — the
      // BFS layout has no vertex positions, so links stay straight there.
      let anyLinkVertices = false;
      const linkData = links
        .map((l, si) => {
          const from = coordMap.get(l.fromId);
          const to = coordMap.get(l.toId);
          if (!from || !to) return null;
          const verts =
            !isSchematic && l.vertices && l.vertices.length > 0
              ? l.vertices
              : null;
          if (verts) anyLinkVertices = true;
          const path: [number, number][] = verts
            ? [from, ...verts, to]
            : [from, to];
          return { ...l, from, to, path, si };
        })
        .filter(Boolean) as Array<
        Link & {
          from: [number, number];
          to: [number, number];
          path: [number, number][];
          si: number;
        }
      >;
      const nodeData = nodes
        .map((n, si) => {
          const position = coordMap.get(n.id);
          if (!position) return null;
          return { ...n, position, si };
        })
        .filter(Boolean) as Array<
        Node & { position: [number, number]; si: number }
      >;
      return {
        linkData,
        nodeData,
        linkDatumById: new Map(linkData.map((l) => [l.id, l])),
        nodeDatumById: new Map(nodeData.map((n) => [n.id, n])),
        anyLinkVertices,
      };
    }, [links, nodes, viewMode, schematicCoords, geoCoords]);

  const buildLayers = useCallback((): Layer[] => {
    const isSchematic = viewMode === "schematic";
    const coordSystem = isSchematic
      ? COORDINATE_SYSTEM.CARTESIAN
      : COORDINATE_SYSTEM.DEFAULT;

    const nodeRadiusUnits = isSchematic
      ? ("common" as const)
      : ("pixels" as const);

    const junctionRadius = 7;
    const specialRadius = 9;

    // Threshold bands only apply in "threshold" colour mode.
    const velThresh =
      colorMode === "threshold" ? velocityThresholds : undefined;
    const flowThresh = colorMode === "threshold" ? flowThresholds : undefined;
    const pressThresh =
      colorMode === "threshold" ? pressureThresholds : undefined;

    // While a node is being dragged (edit tool), patch the dragged node and
    // its incident links into fresh arrays so deck picks up the new
    // positions. Only runs during an active drag — the steady-state path
    // reuses the memoized arrays untouched.
    const drag = draggingNodePosRef.current;
    let ld = linkData;
    let nd = nodeData;
    if (drag) {
      const dragPos: [number, number] = [drag.lng, drag.lat];
      ld = linkData.map((l) => {
        if (l.fromId !== drag.id && l.toId !== drag.id) return l;
        const from = l.fromId === drag.id ? dragPos : l.from;
        const to = l.toId === drag.id ? dragPos : l.to;
        // Only the dragged endpoint moves; intermediate vertices stay fixed.
        return { ...l, from, to, path: [from, ...l.path.slice(1, -1), to] };
      });
      nd = nodeData.map((n) =>
        n.id === drag.id ? { ...n, position: dragPos } : n,
      );
    }
    const linkDatum = (id: string) =>
      drag ? ld.find((l) => l.id === id) : linkDatumById.get(id);
    const nodeDatum = (id: string) =>
      drag ? nd.find((n) => n.id === id) : nodeDatumById.get(id);

    // Period results are flat arrays in network order, looked up by each
    // datum's `si`. Guard against topology changes racing ahead of results.
    const pr =
      periodResult &&
      periodResult.nodePressure.length === nodes.length &&
      periodResult.linkFlow.length === links.length
        ? periodResult
        : null;
    const nodeSim = <T extends Node & { si: number }>(d: T): T =>
      pr
        ? {
            ...d,
            pressure: pr.nodePressure[d.si],
            demand: pr.nodeDemand[d.si],
            head: pr.nodeHead[d.si],
            quality: pr.nodeQuality ? pr.nodeQuality[d.si] : null,
          }
        : d;
    const linkSim = <T extends Link & { si: number }>(d: T): T =>
      pr
        ? {
            ...d,
            flow: pr.linkFlow[d.si],
            velocity: pr.linkVelocity[d.si],
            status: pr.linkStatus[d.si],
            headloss: pr.linkHeadloss[d.si],
            quality: pr.linkQuality ? pr.linkQuality[d.si] : null,
          }
        : d;

    // ── Scenario comparison (Δ overlay) ──
    // Length-guarded like `pr` so topology drift can never pair unrelated
    // elements. In compare mode junction/link colours come from the delta
    // arrays via the diverging ramp; non-junctions keep their type colour,
    // pumps their fixed amber, and the categorical link "status" variable
    // keeps its static colours (a status delta is not meaningful).
    const cmp =
      compare &&
      compare.deltas.nodePressure.length === nodes.length &&
      compare.deltas.linkFlow.length === links.length
        ? compare
        : null;

    // Shared colour accessors — used by BOTH the main node/link layers and
    // the hover/selection glow rings so halos always match the element.
    const nodeColor = (d: (typeof nodeData)[number]): RGBA => {
      if (cmp && d.type === "junction") {
        const field =
          nodeVar === "pressure"
            ? ("nodePressure" as const)
            : nodeVar === "head"
              ? ("nodeHead" as const)
              : nodeVar === "demand"
                ? ("nodeDemand" as const)
                : ("nodeQuality" as const);
        const arr = cmp.deltas[field];
        return divergingRgba(arr ? arr[d.si] : null, cmp.maxAbs[field]);
      }
      return nodeRgba(
        nodeSim(d),
        nodeVar,
        headMin,
        headMax,
        demandMin,
        demandMax,
        qualityMin,
        qualityMax,
        pressThresh,
      );
    };
    const linkColor = (d: (typeof linkData)[number]): RGBA => {
      if (cmp && linkVar !== "status" && d.type !== "pump") {
        const field =
          linkVar === "flow"
            ? ("linkFlow" as const)
            : linkVar === "velocity"
              ? ("linkVelocity" as const)
              : linkVar === "headloss"
                ? ("linkHeadloss" as const)
                : ("linkQuality" as const);
        const arr = cmp.deltas[field];
        return divergingRgba(arr ? arr[d.si] : null, cmp.maxAbs[field]);
      }
      return linkRgba(
        linkSim(d),
        linkVar,
        flowMax,
        velThresh,
        flowThresh,
        qualityMin,
        qualityMax,
      );
    };

    // Three concentric glow rings beneath a hovered/selected link. Returns []
    // when the id is null or has no drawable datum.
    const linkGlowLayers = (
      linkId: string | null,
      idPrefix: string,
      rings: typeof LINK_HOVER_GLOW,
    ): Layer[] => {
      if (!linkId) return [];
      const glowDatum = linkDatum(linkId);
      if (!glowDatum) return [];
      const link = linkSim(glowDatum);
      const [r, g, b] = linkColor(glowDatum);
      const base = {
        coordinateSystem: coordSystem,
        // Same polyline path as the main link layers.
        getPath: (d: typeof link) => d.path,
        widthUnits: "pixels" as const,
        capRounded: true as const,
        jointRounded: true as const,
        pickable: false as const,
        updateTriggers: {},
        data: [link],
      };
      return rings.map(
        ({ suffix, alpha, width }) =>
          new PathLayer({
            ...base,
            id: `${idPrefix}-${suffix}`,
            getColor: [r, g, b, alpha] as unknown as RGBA,
            getWidth: width,
          }),
      );
    };

    // Three concentric glow rings beneath a hovered/selected node.
    const nodeGlowLayers = (
      nodeId: string | null,
      idPrefix: string,
      rings: typeof NODE_HOVER_GLOW,
    ): Layer[] => {
      if (!nodeId) return [];
      const glowDatum = nodeDatum(nodeId);
      if (!glowDatum) return [];
      const node = nodeSim(glowDatum);
      const [r, g, b] = nodeColor(glowDatum);
      const baseR = node.type === "junction" ? junctionRadius : specialRadius;
      const base = {
        coordinateSystem: coordSystem,
        getPosition: (d: typeof node) => d.position,
        radiusUnits: nodeRadiusUnits,
        stroked: false,
        pickable: false as const,
        updateTriggers: {},
        data: [node],
      };
      return rings.map(
        ({ suffix, alpha, radiusPad }) =>
          new ScatterplotLayer({
            ...base,
            id: `${idPrefix}-${suffix}`,
            getRadius: baseR + radiusPad,
            getFillColor: [r, g, b, alpha] as unknown as RGBA,
          }),
      );
    };

    const layers: Layer[] = [];

    if (canvasLayers.model) {
      // ── Glow / halo layers — pushed FIRST so they render beneath links and nodes ──
      // Hover halos are suppressed while the same element is selected.
      layers.push(
        ...linkGlowLayers(
          hoveredLinkId !== selectedLinkId ? hoveredLinkId : null,
          "hover-link-glow",
          LINK_HOVER_GLOW,
        ),
        ...linkGlowLayers(
          selectedLinkId,
          "selection-link-glow",
          LINK_SELECTION_GLOW,
        ),
        ...nodeGlowLayers(
          hoveredNodeId !== selectedNodeId ? hoveredNodeId : null,
          "hover-glow",
          NODE_HOVER_GLOW,
        ),
        ...nodeGlowLayers(
          selectedNodeId,
          "selection-glow",
          NODE_SELECTION_GLOW,
        ),
      );

      // ── Links and nodes — rendered on top of all halos ──
      const onLinkHover = (info: { object?: unknown }) => {
        const id = info.object ? (info.object as { id: string }).id : null;
        hoveredLinkIdRef.current = id;
        setHoveredLinkId(id);
      };
      const onLinkClick = (info: { object?: unknown }) => {
        if (info.object) {
          const id = (info.object as { id: string }).id;
          onSelectLink(id === selectedLinkId ? null : id);
        }
      };
      const linkColorTriggers = [
        linkVar,
        flowMax,
        colorMode,
        velocityThresholds,
        flowThresholds,
        qualityMin,
        qualityMax,
        pr,
        cmp,
      ];
      // Link hover/click is only meaningful in select/edit; skipping the
      // pick pass for other tools halves per-mousemove GPU picking cost.
      const linksPickable = tool === "select" || tool === "edit";
      layers.push(
        // LineLayer cannot render polylines, so networks with link vertices
        // use PathLayer-based variants. Those get their OWN ids
        // ("…-path"): deck.gl matches layers by id alone and transfers the
        // old layer's state (compiled shader model included) into the new
        // instance without checking the class, so a layer class must never
        // change under a reused id. Vertex-free networks keep the cheaper
        // LineLayer fast path under the original ids.
        ...(anyLinkVertices
          ? [
              new PathLayer({
                id: "links-hittarget-path",
                data: ld,
                coordinateSystem: coordSystem,
                getPath: (d) => d.path,
                getColor: [0, 0, 0, 0] as unknown as RGBA,
                getWidth: 12,
                widthUnits: "pixels" as const,
                pickable: linksPickable,
                onHover: onLinkHover,
                onClick: onLinkClick,
                updateTriggers: {},
              }),
            ]
          : [
              new LineLayer({
                id: "links-hittarget",
                data: ld,
                coordinateSystem: coordSystem,
                getSourcePosition: (d) => d.from,
                getTargetPosition: (d) => d.to,
                getColor: [0, 0, 0, 0] as unknown as RGBA,
                getWidth: 12,
                widthUnits: "pixels" as const,
                pickable: linksPickable,
                onHover: onLinkHover,
                onClick: onLinkClick,
                updateTriggers: {},
              }),
            ]),
        // The animated flow layer and the static layers must use distinct
        // ids for the same class-transfer reason as above. FlowPathLayer is
        // already a PathLayer, so it renders the full polyline in both the
        // straight and vertex cases under its single id.
        // Compare mode renders static delta colours — the flow pulse reads
        // absolute velocities/flows and would contradict the Δ ramp.
        ...(animateLinks &&
        cmp == null &&
        (linkVar === "flow" || linkVar === "velocity")
          ? [
              new FlowPathLayer({
                id: "links-flow",
                data: ld,
                coordinateSystem: coordSystem,
                // Geometry is static; flow direction is encoded in the sign
                // of the speed param so reverse flow never re-tesselates.
                getPath: (d) => d.path,
                getColor: linkColor,
                getWidth: 2,
                widthUnits: "pixels" as const,
                capRounded: true,
                jointRounded: true,
                pickable: false,
                flowTime: flowAnimRef.current,
                getFlowParams: (d) => {
                  const l = linkSim(d);
                  const v = l.velocity;
                  const f = l.flow;
                  const speed =
                    v != null && v > 0
                      ? Math.min(1, v / 1.5)
                      : f != null
                        ? Math.min(1, Math.abs(f) / Math.max(0.01, flowMax))
                        : 0.2;
                  const dir = f != null && f < 0 ? -1 : 1;
                  return [speed * dir, 1.0, hashStr(d.id) * 6.28318];
                },
                updateTriggers: {
                  getColor: linkColorTriggers,
                  getFlowParams: [flowMax, pr],
                },
              }),
            ]
          : anyLinkVertices
            ? [
                new PathLayer({
                  id: "links-static-path",
                  data: ld,
                  coordinateSystem: coordSystem,
                  getPath: (d) => d.path,
                  getColor: linkColor,
                  getWidth: 2,
                  widthUnits: "pixels" as const,
                  capRounded: true,
                  jointRounded: true,
                  pickable: false,
                  updateTriggers: {
                    getColor: linkColorTriggers,
                  },
                }),
              ]
            : [
                new LineLayer({
                  id: "links-static",
                  data: ld,
                  coordinateSystem: coordSystem,
                  getSourcePosition: (d) => d.from,
                  getTargetPosition: (d) => d.to,
                  getColor: linkColor,
                  getWidth: 2,
                  widthUnits: "pixels" as const,
                  pickable: false,
                  updateTriggers: {
                    getColor: linkColorTriggers,
                  },
                }),
              ]),
        new ScatterplotLayer({
          id: "nodes",
          data: nd,
          coordinateSystem: coordSystem,
          getPosition: (d) => d.position,
          getFillColor: nodeColor,
          getRadius: (d) =>
            d.type === "junction" ? junctionRadius : specialRadius,
          radiusUnits: nodeRadiusUnits,
          // Measure works on raw map clicks — node picking is dead cost there.
          pickable: tool !== "measure",
          onHover: (info) => {
            const id = info.object ? (info.object as { id: string }).id : null;
            hoveredNodeIdRef.current = id;
            setHoveredNodeId(id);
          },
          onClick: (info) => {
            if (didDragRef.current) {
              didDragRef.current = false;
              return;
            }
            if (toolRef.current === "edit") return;
            if (!info.object) return;
            const id = info.object.id as string;
            if (toolRef.current === "add-link") {
              if (!pendingLinkFromIdRef.current) {
                // First click — record the from-node and highlight it.
                pendingLinkFromIdRef.current = id;
                onSelectNodeRef.current(id);
              } else if (pendingLinkFromIdRef.current === id) {
                // Clicked the same node twice — cancel.
                pendingLinkFromIdRef.current = null;
                ghostLinkRef.current = null;
                onSelectNodeRef.current(null);
              } else {
                // Second click — create the link.
                onCreateLinkRequestRef.current?.(
                  pendingLinkFromIdRef.current,
                  id,
                );
                pendingLinkFromIdRef.current = null;
                ghostLinkRef.current = null;
                onSelectNodeRef.current(null);
              }
              return;
            }
            onSelectNode(id === selectedNodeId ? null : id);
          },
          updateTriggers: {
            getFillColor: [
              nodeVar,
              headMin,
              headMax,
              demandMin,
              demandMax,
              qualityMin,
              qualityMax,
              colorMode,
              pressureThresholds,
              pr,
              cmp,
            ],
            getRadius: [isSchematic],
          },
        }),
      );
    }

    // Labels: cull to the current viewport and cap the count so toggling
    // labels on a 46k network can't freeze layer building (F2). Rebuilds are
    // triggered on map moveend / schematic view changes while labels are on.
    const labelBounds = (() => {
      if (!canvasLayers.nodeLabels && !canvasLayers.linkLabels) return null;
      if (isSchematic) {
        const vs = viewStateRef.current as SchematicViewState;
        if (!vs || !("target" in vs)) return null;
        const w = containerRef.current?.clientWidth ?? 1200;
        const h = containerRef.current?.clientHeight ?? 800;
        const scale = 2 ** vs.zoom;
        const hw = w / 2 / scale;
        const hh = h / 2 / scale;
        return {
          minX: vs.target[0] - hw,
          maxX: vs.target[0] + hw,
          minY: vs.target[1] - hh,
          maxY: vs.target[1] + hh,
        };
      }
      const b = mapRef.current?.getBounds();
      if (!b) return null;
      return {
        minX: b.getWest(),
        maxX: b.getEast(),
        minY: b.getSouth(),
        maxY: b.getNorth(),
      };
    })();
    const inBounds = (x: number, y: number) =>
      labelBounds != null &&
      x >= labelBounds.minX &&
      x <= labelBounds.maxX &&
      y >= labelBounds.minY &&
      y <= labelBounds.maxY;
    const capLabels = <T,>(items: T[]): T[] =>
      items.length > MAX_LABELS ? [] : items;

    if (canvasLayers.nodeLabels) {
      const labelNodes = capLabels(
        nd.filter((n) => inBounds(n.position[0], n.position[1])),
      );
      layers.push(
        new TextLayer({
          id: "labels-nodes",
          data: labelNodes,
          coordinateSystem: coordSystem,
          getPosition: (d) => d.position,
          getText: (d) => d.id,
          getSize: isSchematic ? 9 : 11,
          getColor: [255, 255, 255, 140] as unknown as RGBA,
          getPixelOffset: [0, isSchematic ? 12 : 16],
          background: false,
          fontFamily: "monospace",
        }),
      );
    }

    if (canvasLayers.linkLabels) {
      const labelLinks = capLabels(
        ld.filter(
          (l) => inBounds(l.from[0], l.from[1]) || inBounds(l.to[0], l.to[1]),
        ),
      );
      layers.push(
        new TextLayer({
          id: "labels-links",
          data: labelLinks,
          coordinateSystem: coordSystem,
          // Deliberately the from/to chord midpoint (not the polyline
          // midpoint): cheap, stable across vertex edits, and close enough
          // for a floating id label.
          getPosition: (d) =>
            [(d.from[0] + d.to[0]) / 2, (d.from[1] + d.to[1]) / 2] as [
              number,
              number,
            ],
          getText: (d) => d.id,
          getSize: isSchematic ? 8 : 10,
          getColor: [255, 255, 200, 130] as unknown as RGBA,
          background: false,
          fontFamily: "monospace",
        }),
      );
    }

    // Ghost link drawn while in add-link mode after the first node is picked.
    const ghost = ghostLinkRef.current;
    if (ghost) {
      layers.push(
        new LineLayer({
          id: "ghost-link",
          data: [ghost],
          coordinateSystem: coordSystem,
          getSourcePosition: (d) => d.from,
          getTargetPosition: (d) => d.to,
          getColor: [255, 255, 255, 180] as unknown as RGBA,
          getWidth: 2,
          widthUnits: "pixels",
          getDashArray: [6, 4],
          extensions: [],
          pickable: false,
        }) as unknown as Layer,
      );
    }

    // Measure rubber-band: anchor dot + dashed line to cursor.
    const mAnchor = measureAnchorRef.current;
    const mCursor = measureCursorRef.current;
    if (mAnchor) {
      layers.push(
        new ScatterplotLayer({
          id: "measure-anchor",
          data: [mAnchor],
          coordinateSystem: coordSystem,
          getPosition: (d) => d,
          getRadius: 5,
          radiusUnits: "pixels",
          getFillColor: [212, 160, 23, 255] as unknown as RGBA,
          getLineColor: [0, 0, 0, 180] as unknown as RGBA,
          stroked: true,
          lineWidthUnits: "pixels",
          getLineWidth: 1,
          pickable: false,
        }) as unknown as Layer,
      );
      if (mCursor) {
        layers.push(
          new LineLayer({
            id: "measure-line",
            data: [{ from: mAnchor, to: mCursor }],
            coordinateSystem: coordSystem,
            getSourcePosition: (d) => d.from,
            getTargetPosition: (d) => d.to,
            getColor: [212, 160, 23, 200] as unknown as RGBA,
            getWidth: 2,
            widthUnits: "pixels",
            pickable: false,
          }) as unknown as Layer,
        );
        layers.push(
          new ScatterplotLayer({
            id: "measure-cursor",
            data: [mCursor],
            coordinateSystem: coordSystem,
            getPosition: (d) => d,
            getRadius: 5,
            radiusUnits: "pixels",
            getFillColor: [212, 160, 23, 255] as unknown as RGBA,
            getLineColor: [0, 0, 0, 180] as unknown as RGBA,
            stroked: true,
            lineWidthUnits: "pixels",
            getLineWidth: 1,
            pickable: false,
          }) as unknown as Layer,
        );
      }
    }

    return layers;
  }, [
    linkData,
    nodeData,
    linkDatumById,
    nodeDatumById,
    anyLinkVertices,
    periodResult,
    compare,
    nodes,
    links,
    viewMode,
    nodeVar,
    linkVar,
    animateLinks,
    headMin,
    headMax,
    demandMin,
    demandMax,
    flowMax,
    qualityMin,
    qualityMax,
    canvasLayers,
    selectedNodeId,
    onSelectNode,
    selectedLinkId,
    onSelectLink,
    hoveredNodeId,
    hoveredLinkId,
    tool,
    colorMode,
    pressureThresholds,
    velocityThresholds,
    flowThresholds,
  ]);

  useEffect(() => {
    buildLayersRef.current = buildLayers;
  }, [buildLayers]);

  // Viewport-culled labels need a layer rebuild when the view moves. Tracked
  // via refs + a rAF so pan/zoom with labels off costs nothing.
  const labelsOnRef = useRef(false);
  useEffect(() => {
    labelsOnRef.current = canvasLayers.nodeLabels || canvasLayers.linkLabels;
  }, [canvasLayers]);
  const labelRefreshRafRef = useRef<number | null>(null);
  const scheduleLabelRefresh = useCallback((mode: "map" | "schematic") => {
    if (labelRefreshRafRef.current != null) return;
    labelRefreshRafRef.current = requestAnimationFrame(() => {
      labelRefreshRafRef.current = null;
      const layers = buildLayersRef.current();
      if (mode === "map") overlayRef.current?.setProps({ layers });
      else deckRef.current?.setProps({ layers });
    });
  }, []);

  // Clear the drag-position override once geoCoords has been rebuilt with the
  // updated coordinates from the backend.  Keying on geoCoords (not nodes)
  // ensures the new coordMap is in place before buildLayers uses it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `geoCoords` is an intentional trigger to clear the drag override once the backend has updated coordinates.
  useEffect(() => {
    draggingNodePosRef.current = null;
  }, [geoCoords]);

  const ensureDeck = useCallback(() => {
    if (deckRef.current || !deckHostRef.current) return deckRef.current;
    const initialViewState = orthoCenterFromMap(schematicCoordsRef.current);
    viewStateRef.current = initialViewState;
    const deck = new Deck({
      parent: deckHostRef.current,
      style: { position: "absolute", inset: "0", zIndex: "1" },
      views: orthoViewRef.current,
      viewState: initialViewState,
      controller: true,
      pickingRadius: 6,
      onViewStateChange: ({
        viewState,
      }: ViewStateChangeParameters<OrthographicViewState>) => {
        const nextViewState: SchematicViewState = {
          target: viewState.target as [number, number, number],
          zoom: Number(viewState.zoom ?? 0),
        };
        viewStateRef.current = nextViewState;
        deckRef.current?.setProps({ viewState: nextViewState });
        // Labels are viewport-culled; refresh them as the view moves.
        if (labelsOnRef.current) scheduleLabelRefresh("schematic");
      },
      layers: [],
    });
    deckRef.current = deck;
    deckCanvasRef.current = deck.getCanvas();
    if (deckCanvasRef.current) {
      deckCanvasRef.current.style.background = "transparent";
      deckCanvasRef.current.style.display =
        viewMode === "schematic" ? "" : "none";
    }
    return deck;
  }, [viewMode, scheduleLabelRefresh]);

  useEffect(() => {
    if (!mapElRef.current) return;

    const initialVs = roughGeoViewState(nodesRef.current);
    const map = new maplibregl.Map({
      container: mapElRef.current,
      // Read the style via the ref, NOT the `basemap` prop: having `basemap`
      // in this effect's deps tears down and recreates the whole map (losing
      // the viewport) on every style switch — the setStyle effect below
      // handles changes in place.
      style: MAP_STYLES[prevBasemapRef.current],
      center: [initialVs.longitude, initialVs.latitude],
      zoom: initialVs.zoom,
      attributionControl: false,
    });
    mapRef.current = map;

    map.on("moveend", () => {
      if (labelsOnRef.current && viewModeRef.current === "map") {
        scheduleLabelRefresh("map");
      }
    });

    map.on("style.load", () => {
      // setStyle tears down style-owned layers/sources. Reattach and reapply
      // the deck overlay so network features remain visible after basemap switches.
      const overlay = overlayRef.current;
      if (overlay) {
        try {
          map.removeControl(overlay);
        } catch {
          /* ignore */
        }
        try {
          map.addControl(overlay);
        } catch {
          /* ignore */
        }
      }
      if (isActiveRef.current && viewModeRef.current === "map") {
        overlayRef.current?.setProps({ layers: buildLayersRef.current() });
        markFirstFrame("map");
      }
      fitMapExtents(nodesRef.current, map);
    });

    const overlay = new MapboxOverlay({ layers: [], pickingRadius: 6 });
    map.addControl(overlay);
    overlayRef.current = overlay;

    map.on("mousedown", (e) => {
      if (toolRef.current !== "edit") return;
      const nodeId = hoveredNodeIdRef.current;
      if (!nodeId) return;
      didDragRef.current = false;
      // A previous drop's fallback timer would clear this new drag's position
      // override mid-drag — cancel it.
      if (dragFallbackTimerRef.current != null) {
        window.clearTimeout(dragFallbackTimerRef.current);
        dragFallbackTimerRef.current = null;
      }
      draggingNodeIdRef.current = nodeId;
      draggingNodePosRef.current = {
        id: nodeId,
        lng: e.lngLat.lng,
        lat: e.lngLat.lat,
      };
      map.dragPan.disable();
      map.getCanvas().style.cursor = "grabbing";
      // Do not open the inspector while in move/edit mode.
    });
    map.on("mousemove", (e) => {
      if (viewModeRef.current !== "map") return;
      const { lng, lat } = e.lngLat;
      if (draggingNodeIdRef.current) {
        didDragRef.current = true;
        draggingNodePosRef.current = {
          id: draggingNodeIdRef.current,
          lng,
          lat,
        };
        overlayRef.current?.setProps({ layers: buildLayersRef.current() });
        return;
      }
      if (toolRef.current === "add-link" && pendingLinkFromIdRef.current) {
        const fromCoords = geoCoordsRef.current.get(
          pendingLinkFromIdRef.current,
        );
        if (fromCoords) {
          ghostLinkRef.current = { from: fromCoords, to: [lng, lat] };
          overlayRef.current?.setProps({ layers: buildLayersRef.current() });
        }
      }
      if (toolRef.current === "measure") {
        measureCursorRef.current = [lng, lat];
        overlayRef.current?.setProps({ layers: buildLayersRef.current() });
      }
    });
    map.on("mouseup", (e) => {
      if (!draggingNodeIdRef.current) return;
      const nodeId = draggingNodeIdRef.current;
      draggingNodeIdRef.current = null;
      // Keep draggingNodePosRef set so buildLayers continues to show the dropped
      // position until the parent re-renders with updated coordinates from the backend.
      map.dragPan.enable();
      map.getCanvas().style.cursor = "";
      onNodeMovedRef.current?.(nodeId, e.lngLat.lng, e.lngLat.lat);
      // Failed/absent position patches never refresh geoCoords, which is what
      // normally clears the drag override — without this fallback the drag
      // branch of buildLayers (fresh 46k arrays per frame) stays pinned on.
      dragFallbackTimerRef.current = window.setTimeout(() => {
        dragFallbackTimerRef.current = null;
        if (!draggingNodeIdRef.current && draggingNodePosRef.current) {
          draggingNodePosRef.current = null;
          overlayRef.current?.setProps({ layers: buildLayersRef.current() });
        }
      }, 5000);
    });
    // Releasing the button outside the map canvas (over a panel, outside the
    // window) never fires map "mouseup" — the drag stayed armed with dragPan
    // disabled. Cancel it: restore the node and re-enable panning.
    const onWindowPointerUp = () => {
      if (!draggingNodeIdRef.current) return;
      draggingNodeIdRef.current = null;
      draggingNodePosRef.current = null;
      map.dragPan.enable();
      map.getCanvas().style.cursor = "";
      overlayRef.current?.setProps({ layers: buildLayersRef.current() });
    };
    window.addEventListener("pointerup", onWindowPointerUp);
    map.on("click", (e) => {
      const { lng, lat } = e.lngLat;
      if (toolRef.current === "measure") {
        if (!measureAnchorRef.current) {
          // First click — set anchor, clear any stale cursor.
          measureAnchorRef.current = [lng, lat];
          measureCursorRef.current = null;
        } else {
          // Second click — report and reset for next measurement.
          onMeasurePointRef.current?.(lng, lat);
          measureAnchorRef.current = null;
          measureCursorRef.current = null;
        }
        overlayRef.current?.setProps({ layers: buildLayersRef.current() });
        return;
      }
      if (toolRef.current !== "add-node") return;
      if (hoveredNodeIdRef.current || hoveredLinkIdRef.current) return;
      onCreateNodeRequestRef.current?.(lng, lat);
    });

    return () => {
      if (dragFallbackTimerRef.current != null) {
        window.clearTimeout(dragFallbackTimerRef.current);
        dragFallbackTimerRef.current = null;
      }
      try {
        map.removeControl(overlay);
      } catch {
        /* ignore */
      }
      try {
        deckRef.current?.finalize();
      } catch {
        /* ignore */
      }
      try {
        map.remove();
      } catch {
        /* ignore */
      }
      window.removeEventListener("pointerup", onWindowPointerUp);
      overlayRef.current = null;
      deckRef.current = null;
      deckCanvasRef.current = null;
      mapRef.current = null;
    };
  }, [markFirstFrame, scheduleLabelRefresh]);

  useEffect(() => {
    if (!isActive) return;
    if (viewMode !== "schematic") return;
    const deck = ensureDeck();
    if (!deck) return;
    const { target, zoom } = orthoCenterFromMap(schematicCoords);
    const vs = { target, zoom };
    viewStateRef.current = vs;
    deck.setProps({
      views: orthoViewRef.current,
      viewState: vs,
      layers: buildLayersRef.current(),
    });
    markFirstFrame("schematic");
    if (deckCanvasRef.current) deckCanvasRef.current.style.display = "";
  }, [ensureDeck, isActive, markFirstFrame, schematicCoords, viewMode]);

  useEffect(() => {
    const deck = deckRef.current;
    if (!isActive || !deck || viewMode !== "schematic") return;
    deck.setProps({ layers: buildLayers(), viewState: viewStateRef.current });
    markFirstFrame("schematic");
  }, [buildLayers, isActive, markFirstFrame, viewMode]);

  // Compare mode forces the flow/velocity pulse off (static Δ colours only);
  // buildLayers applies the same gate when picking the flow layer class.
  const linkAnimationActive =
    animateLinks &&
    compare == null &&
    (linkVar === "flow" || linkVar === "velocity");

  // Flow-animation loop — one RAF effect drives both view modes, pushing
  // fresh layers to the schematic deck or the map overlay. The clock resets
  // whenever the schematic loop is not running (inactive tab, animation off,
  // or map mode); this matches the previous per-mode effects, where the
  // schematic effect reset the clock in exactly those states and the map loop
  // then advanced it from zero.
  useEffect(() => {
    if (!isActive || !linkAnimationActive || viewMode !== "schematic") {
      flowAnimRef.current = 0;
    }
    if (!isActive || !linkAnimationActive) return;
    const isSchematic = viewMode === "schematic";
    let rafId: number;
    let lastTs = performance.now();
    function tick(now: number) {
      const dt = Math.min(now - lastTs, 50);
      lastTs = now;
      flowAnimRef.current = (flowAnimRef.current + dt * 0.001) % 3600;
      const layers = buildLayersRef.current();
      if (isSchematic) deckRef.current?.setProps({ layers });
      else overlayRef.current?.setProps({ layers });
      rafId = requestAnimationFrame(tick);
    }
    rafId = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafId);
  }, [isActive, linkAnimationActive, viewMode]);

  // Update overlay when data/layers change in map mode.
  useEffect(() => {
    if (!isActive || viewMode !== "map") return;
    overlayRef.current?.setProps({ layers: buildLayers() });
    markFirstFrame("map");
  }, [buildLayers, isActive, markFirstFrame, viewMode]);

  // Deliberately KEEP layers mounted while inactive: every setProps path and
  // the flow-animation RAF loop are isActive-gated, so a hidden canvas does zero work, and
  // retaining deck's attribute buffers makes switching back to the Canvas tab
  // near-instant. Dropping layers here previously forced a full accessor +
  // tesselation + GPU upload rebuild (~100-400ms at 46k) per re-activation.
  // (If the network changed while hidden, the re-activation effects push the
  // updated layers as usual.)

  // Basemap style change — MapboxOverlay re-attaches automatically as IControl.
  useEffect(() => {
    if (!isActive) return;
    const map = mapRef.current;
    if (!map) return;
    if (prevBasemapRef.current === basemap) return;
    prevBasemapRef.current = basemap;
    map.setStyle(MAP_STYLES[basemap]);
  }, [basemap, isActive]);

  // View mode switch.
  useEffect(() => {
    if (!isActive) return;
    const enteringMapMode =
      viewMode === "map" && prevViewModeRef.current !== "map";
    prevViewModeRef.current = viewMode;

    if (viewMode === "schematic") {
      // Clear overlay when entering schematic so no map-mode layer lingers.
      overlayRef.current?.setProps({ layers: [] });
      if (mapElRef.current) mapElRef.current.style.display = "none";
      if (deckCanvasRef.current) deckCanvasRef.current.style.display = "";
      if (deckHostRef.current) deckHostRef.current.style.pointerEvents = "";
      return;
    }

    // Entering map mode.
    if (deckRef.current) deckRef.current.setProps({ layers: [] });
    if (deckCanvasRef.current) deckCanvasRef.current.style.display = "none";
    if (deckHostRef.current) deckHostRef.current.style.pointerEvents = "none";
    if (mapElRef.current) mapElRef.current.style.display = "";
    if (enteringMapMode) {
      const map = mapRef.current;
      if (map) fitMapExtents(nodesRef.current, map);
    }
  }, [isActive, viewMode]);

  // ── Fit-to-network: fires when nodes first arrive (initial load) or when
  //    fitKey changes (explicit project switch).  Does NOT fire on scenario
  //    switches so the user's chosen view position is preserved.
  const prevHasNodesRef = useRef(nodes.length > 0);
  const prevFitKeyRef = useRef(fitKey);
  useEffect(() => {
    if (!isActive) return;
    const hasNodes = nodes.length > 0;
    const nodesJustArrived = hasNodes && !prevHasNodesRef.current;
    const fitKeyChanged = fitKey !== prevFitKeyRef.current;
    prevHasNodesRef.current = hasNodes;
    prevFitKeyRef.current = fitKey;

    if (!hasNodes) return;
    if (!nodesJustArrived && !fitKeyChanged) return;

    if (viewMode === "schematic") {
      const deck = ensureDeck();
      if (!deck) return;
      const { target, zoom } = orthoCenterFromMap(schematicCoords);
      const vs = { target, zoom };
      viewStateRef.current = vs;
      deck.setProps({
        views: orthoViewRef.current,
        viewState: vs,
        layers: buildLayers(),
      });
    } else {
      const map = mapRef.current;
      if (!map) return;
      const bounds = geoBounds(nodes);
      if (bounds) {
        fitMapExtents(nodes, map);
      } else {
        map.jumpTo({ center: [0, 20], zoom: 1 });
      }
    }
  }, [
    buildLayers,
    ensureDeck,
    fitKey,
    isActive,
    nodes,
    schematicCoords,
    viewMode,
  ]);

  // ── Generic viewport controls (zoom +/- and north reset) ───────────────
  const prevZoomInKeyRef = useRef(zoomInKey);
  const prevZoomOutKeyRef = useRef(zoomOutKey);
  const prevResetNorthKeyRef = useRef(resetNorthKey);
  useEffect(() => {
    if (!isActive) return;
    const zoomInChanged = zoomInKey !== prevZoomInKeyRef.current;
    const zoomOutChanged = zoomOutKey !== prevZoomOutKeyRef.current;
    const resetNorthChanged = resetNorthKey !== prevResetNorthKeyRef.current;
    prevZoomInKeyRef.current = zoomInKey;
    prevZoomOutKeyRef.current = zoomOutKey;
    prevResetNorthKeyRef.current = resetNorthKey;

    // Zoom one step in the active view. Map clamps to [0, 22]; schematic
    // (log2 orthographic zoom) clamps to [-6, 12]. Returns false only when
    // the schematic deck is unavailable.
    const zoomStep = (dir: 1 | -1): boolean => {
      if (viewMode === "map") {
        const map = mapRef.current;
        if (map) {
          map.easeTo({
            zoom:
              dir === 1
                ? Math.min(22, map.getZoom() + 1)
                : Math.max(0, map.getZoom() - 1),
            duration: 220,
          });
        }
        return true;
      }
      const deck = ensureDeck();
      if (!deck) return false;
      const current = viewStateRef.current as SchematicViewState;
      const vs = {
        target: current.target,
        zoom:
          dir === 1
            ? Math.min(12, Number(current.zoom ?? 0) + 1)
            : Math.max(-6, Number(current.zoom ?? 0) - 1),
      };
      viewStateRef.current = vs;
      deck.setProps({ viewState: vs });
      return true;
    };

    if (zoomInChanged && !zoomStep(1)) return;
    if (zoomOutChanged && !zoomStep(-1)) return;

    if (resetNorthChanged && viewMode === "map") {
      mapRef.current?.easeTo({ bearing: 0, pitch: 0, duration: 260 });
    }
  }, [ensureDeck, isActive, resetNorthKey, viewMode, zoomInKey, zoomOutKey]);

  return (
    <div
      ref={containerRef}
      style={{
        position: "absolute",
        inset: 0,
        cursor:
          hoveredNodeId != null || hoveredLinkId != null
            ? "pointer"
            : "default",
      }}
      onPointerLeave={() => {
        hoveredNodeIdRef.current = null;
        setHoveredNodeId(null);
        hoveredLinkIdRef.current = null;
        setHoveredLinkId(null);
      }}
    >
      <div ref={mapElRef} style={{ position: "absolute", inset: 0 }} />
      <div ref={deckHostRef} style={{ position: "absolute", inset: 0 }} />
    </div>
  );
});
