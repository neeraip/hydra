import type {
  Layer,
  OrthographicViewState,
  ViewStateChangeParameters,
} from "@deck.gl/core";
import { COORDINATE_SYSTEM, Deck, OrthographicView } from "@deck.gl/core";
import {
  LineLayer,
  PathLayer,
  PolygonLayer,
  ScatterplotLayer,
  TextLayer,
} from "@deck.gl/layers";
import { MapboxOverlay } from "@deck.gl/mapbox";
// maplibre-gl 6 dropped its default export; the namespace import keeps
// every `maplibregl.X` usage unchanged.
import * as maplibregl from "maplibre-gl";
// maplibre-gl 6 loads its worker from a real URL resolved against
// import.meta.url — a resolution that returns "" for non-http(s) origins
// (the packaged app is served from tauri://localhost) and would point at a
// file Vite never emits even where the scheme passes. Bundle the worker
// entry explicitly and hand its emitted URL to maplibre, or every vector
// source silently dies in production builds.
import maplibreWorkerUrl from "maplibre-gl/dist/maplibre-gl-worker.mjs?worker&url";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Link, Node, PeriodResults } from "../hooks";
import {
  type BasemapProvider,
  useBasemapProviders,
} from "../hooks/basemapProviders";
import { startPerfSpan } from "../perfTrace";
import type { Region } from "../types";
import { useUnitSystem } from "../units";
import {
  type BasemapId,
  buildProviderRasterStyle,
  parseProviderBasemapId,
} from "./Basemap";
import { FlowPathLayer } from "./FlowPathLayer";
import { HoverChip, type HoverTip } from "./HoverChip";
import { useHoverActions, useHoverState } from "./hover-context";
import { useCanvasLayers } from "./layers-context";
import {
  pulseApplies,
  pulseKindFor,
  pulsePattern,
  pulseSpeed,
} from "./linkPulse";
import {
  baseLinkRgba,
  baseNodeRgba,
  genericRgba,
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
  visibleMapPadding,
} from "./MapCanvas/geoUtils";
import { nearestPointOnPath, type SnapResult } from "./measureSnap";
import {
  NODE_SCALE_DEFAULT,
  nodeRadius,
  nodeRadiusTrigger,
  nodeScaleFactor,
  typicalLinkLength,
} from "./nodeScale";
import {
  computeSchematicLayout,
  type LayoutCoupling,
  type SchematicLayout,
} from "./schematicLayout";
import {
  pathIntersectsBox,
  pointInBox,
  ringIntersectsBox,
  useViewportActions,
  type ViewportBox,
} from "./viewport-context";

/** Stable empty default so the prop never changes identity per render. */
const EMPTY_COUPLINGS: LayoutCoupling[] = [];

import type {
  CanvasTool,
  GenericCanvasResults,
  LinkVariable,
  NodeVariable,
  ViewMode,
} from "./types";

maplibregl.setWorkerUrl(maplibreWorkerUrl);

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
const MAP_STYLES: Record<string, string | maplibregl.StyleSpecification> = {
  streets: "https://tiles.openfreemap.org/styles/liberty",
  light: "https://tiles.openfreemap.org/styles/positron",
  dark: "https://tiles.openfreemap.org/styles/dark",
  none: BLANK_STYLE,
};

/**
 * Resolve a basemap id to a MapLibre style. Legacy ids map to the constants
 * above; `provider:{providerId}:{styleId}` ids build a raster style on the
 * fly from the live catalog. Unknown/broken ids (stale prefs, disconnected
 * catalog) fall back to "streets" gracefully.
 */
function resolveBasemapStyle(
  basemap: BasemapId,
  providers: readonly BasemapProvider[],
): string | maplibregl.StyleSpecification {
  const legacy = MAP_STYLES[basemap];
  if (legacy !== undefined) return legacy;
  const parsed = parseProviderBasemapId(basemap);
  if (parsed) {
    const provider = providers.find(
      (p) => p.id === parsed.providerId && !p.builtin,
    );
    const style = provider?.styles.find((s) => s.id === parsed.styleId);
    if (provider && style) {
      return buildProviderRasterStyle({
        providerId: provider.id,
        styleId: style.id,
        tileSize: style.tileSize,
        maxZoom: style.maxZoom,
        attribution: provider.attribution,
      });
    }
  }
  return MAP_STYLES.streets;
}

/** True when two resolved styles would render identically (style URLs compare
 * by value; built specs are tiny, so structural JSON equality is cheap). */
function sameBasemapStyle(
  a: string | maplibregl.StyleSpecification,
  b: string | maplibregl.StyleSpecification,
): boolean {
  if (a === b) return true;
  if (typeof a === "string" || typeof b === "string") return false;
  return JSON.stringify(a) === JSON.stringify(b);
}

const EMPTY_SCHEMATIC_LAYOUT: SchematicLayout = {
  positions: new Map(),
  detachedIds: new Set(),
  regionRings: new Map(),
  regionLeaders: new Map(),
};
/** Stable empties for hidden layers, so toggling visibility off does not hand
 * deck.gl a fresh array identity on every rebuild. */
const EMPTY_LINK_DATA: never[] = [];
const EMPTY_REGION_DATA: never[] = [];
const EMPTY_NODE_DATA: never[] = [];

/** Stable default so map-mode callers omitting the prop never invalidate the
 * schematic layout cache. */
const IDENTITY_SCALE = { x: 1, y: 1 } as const;

// Glow/halo ring tables (outer → inner) for the hover/selection highlight
// layers built in buildLayers. Alphas/widths/radius pads are visual tuning —
// the layer ids derived from these suffixes must stay stable (deck.gl matches
// layers by id).
// Hover halo. Achromatic on purpose, for the same reason the measure
// highlight below is: a halo tinted with the element's own colour separates
// by hue, and there is no hue an element cannot already be.
//
// It failed asymmetrically, which is why it survived. Around a thin link a
// same-coloured halo is many times the line's width and reads fine; around
// a filled disc it is a narrow annulus in the disc's own colour, at 7–27%
// alpha, on a map where those discs sit shoulder to shoulder — invisible.
// One interaction should not look like two, so both use the rim.
const HOVER_HALO_RGB: [number, number, number] = [226, 232, 242];
const LINK_HOVER_GLOW = [
  { suffix: "outer", alpha: 20, width: 18, rgb: HOVER_HALO_RGB },
  { suffix: "mid", alpha: 60, width: 9, rgb: HOVER_HALO_RGB },
  { suffix: "inner", alpha: 110, width: 4, rgb: HOVER_HALO_RGB },
];
const LINK_SELECTION_GLOW = [
  { suffix: "outer", alpha: 40, width: 22 },
  { suffix: "mid", alpha: 90, width: 10 },
  { suffix: "inner", alpha: 170, width: 5 },
];
// Alphas above the link's: a node's halo is only the annulus outside its
// own radius, so less of it is visible for the same nominal strength.
const NODE_HOVER_GLOW = [
  { suffix: "outer", alpha: 45, radiusPad: 14, rgb: HOVER_HALO_RGB },
  { suffix: "mid", alpha: 95, radiusPad: 8, rgb: HOVER_HALO_RGB },
  { suffix: "inner", alpha: 150, radiusPad: 4, rgb: HOVER_HALO_RGB },
];
const NODE_SELECTION_GLOW = [
  { suffix: "outer", alpha: 35, radiusPad: 18 },
  { suffix: "mid", alpha: 80, radiusPad: 11 },
  { suffix: "inner", alpha: 140, radiusPad: 5 },
];

// Measure-mode snap highlight. Deliberately not a tint of the element's own
// colour, and deliberately not the tool's amber: both pressure and velocity
// ramps run through yellow, so an amber halo on an amber element was invisible.
// A dark→light→dark sandwich reads against any fill and either basemap, because
// it separates by luminance rather than by hue — there is no element colour it
// can collide with.
const MEASURE_HALO_DARK: [number, number, number] = [10, 12, 16];
const MEASURE_HALO_LIGHT: [number, number, number] = [255, 255, 255];
const NODE_MEASURE_GLOW = [
  { suffix: "outer", alpha: 130, radiusPad: 13, rgb: MEASURE_HALO_DARK },
  { suffix: "mid", alpha: 230, radiusPad: 8, rgb: MEASURE_HALO_LIGHT },
  { suffix: "inner", alpha: 150, radiusPad: 3, rgb: MEASURE_HALO_DARK },
];
const LINK_MEASURE_GLOW = [
  { suffix: "outer", alpha: 130, width: 16, rgb: MEASURE_HALO_DARK },
  { suffix: "mid", alpha: 230, width: 10, rgb: MEASURE_HALO_LIGHT },
  { suffix: "inner", alpha: 150, width: 5, rgb: MEASURE_HALO_DARK },
];

// ── Element sizing: world-space, zoom-scaled, pixel-clamped ──────────────────
//
// Node radius and link width are expressed in world units (metres on the geo
// map, "common" units in the orthographic schematic) so the GPU scales them
// with zoom for free — no per-frame relayout on large networks. These pixel
// clamps keep elements visible when zoomed out and stop them ballooning when
// zoomed in. Base sizes are the literals at each layer (junction/special
// radius, link/hit width); tune these bounds to taste.
/** Grab radius for measure snapping. Node dots bottom out at
 * `NODE_RADIUS_MIN_PX`, so without a radius they would be near-unclickable on a
 * whole-network view. */
/** Padding around the detached group's bounding box, as a share of its own
 * extent, with a floor for the single-node case. */
const DETACHED_BOX_PAD_FRACTION = 0.18;
const DETACHED_BOX_MIN_PAD = 50;
const MEASURE_SNAP_RADIUS_PX = 10;
/** Stable empty default so omitting the prop never invalidates a layer diff. */
const EMPTY_MEASURE_POINTS: readonly [number, number][] = [];
const MEASURE_AMBER: [number, number, number, number] = [212, 160, 23, 255];
const NODE_RADIUS_MIN_PX = 2.5;
const NODE_RADIUS_MAX_PX = 13;
const NODE_GLOW_MAX_PX = 26;
const LINK_WIDTH_MIN_PX = 2.5;
const LINK_WIDTH_MAX_PX = 9;
const LINK_GLOW_MAX_PX = 28;
const LINK_HIT_MIN_PX = 8;
const LINK_HIT_MAX_PX = 28;

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
  /** Areal elements (subcatchment boundaries), already in the render CRS.
   * Drawn from their own rings wherever positions are real; in a topological
   * layout they are drawn from the glyphs that layout placed. */
  regions?: Region[];
  viewMode: ViewMode;
  /** Per-axis schematic spacing multipliers (`{x: 1, y: 1}` = the layout's
   * native 120:80). Scales distances between nodes only — radii and link widths
   * are layer properties and are deliberately untouched. Only the *ratio*
   * matters: scaling both equally is arithmetically the same as zooming, and
   * the camera refit below removes it. Ignored in map mode. */
  schematicScale?: { x: number; y: number };
  /** Node-size slider position; the midpoint is the size derived from the
   *  network's own spacing. See `nodeScale`. */
  nodeScale?: number;
  nodeVar: NodeVariable;
  linkVar: LinkVariable;
  /** Animate the Flow/Velocity pulse effect. Already accounts for the user
   * toggle and the "Reduce motion" accessibility setting. */
  animateLinks?: boolean;
  /** Flat per-period result arrays (network order). Passed separately from
   * nodes/links so a timeline scrub changes only this prop — the node/link
   * arrays keep their identity and deck.gl only re-evaluates colours. */
  periodResult?: PeriodResults | null;
  /** Engine-generic result channels (catalog-driven engines). When present,
   * node/link/region colours come from these values and ramps instead of
   * the fixed-variable accessors — the canvas stays free of engine
   * knowledge; the engine's catalog described everything. */
  generic?: GenericCanvasResults | null;
  basemap: BasemapId;
  /** Basemap dimming, 0–1 (1 = fully opaque). Applied as CSS opacity on the
   * maplibre canvas only — never on the deck.gl network overlay. */
  basemapOpacity?: number;
  selectedNodeId: string | null;
  onSelectNode: (id: string | null) => void;
  selectedLinkId: string | null;
  onSelectLink: (id: string | null) => void;
  /** The model's coordinates are a local grid, not a georeferenced system:
   * there is no basemap to place them on, so the canvas renders them
   * orthographically at their true positions. */
  /**
   * Whether the positions on screen are *invented* by the layout rather than
   * the model's own.
   *
   * This is not the same question as `viewMode`, which selects the rendering
   * path (maplibre + basemap, or the orthographic deck view). A local grid
   * renders orthographically — it has no basemap to sit on — while its
   * coordinates are entirely real. Conflating the two is what made a local
   * grid's plan view masquerade as the schematic, and made the real
   * topological layout unreachable for exactly the models that need it most.
   */
  /** Kind id → the role the engine declared for it (spec §4.3). Drives the
   * network-at-rest palette, so an unsimulated model shows where it is fed
   * and drained rather than one uniform grey. */
  kindRoles?: ReadonlyMap<string, string>;
  topological?: boolean;
  /**
   * Whether the inlet couplings have been answered for.
   *
   * They are hydraulic connections that are not links, so a topological
   * layout computed before they arrive reads a dual-drainage model as two
   * disconnected networks — and says so, loudly, in an amber detached
   * group, then reframes when the answer lands. Waiting costs one blank
   * frame; not waiting costs a wrong one that the camera then fits to.
   */
  couplingsResolved?: boolean;
  /** Hydraulic connections that are not links (dual-drainage street
   * inlets): drawn as dashed connectors and counted as connectivity by
   * the schematic layout. */
  couplings?: LayoutCoupling[];
  /** Selected areal element; highlights its ring. */
  selectedRegionId?: string | null;
  /** Called when a region polygon is clicked (select tool, map mode). */
  onSelectRegion?: (id: string | null) => void;
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
  /** Run ranges for the magnitude ramps pressure and velocity take when the
   * reader has not asked for the verdict. */
  pressureMin?: number;
  pressureMax?: number;
  velocityMax?: number;
  /** Active canvas tool; affects cursor and interaction mode. */
  tool?: CanvasTool;
  /** Called (after mouseup) when the user drags a node to a new position.
   * `x` and `y` are geographic coordinates (longitude and latitude).
   * Return (or resolve) `false` to signal the move was NOT committed — the
   * canvas immediately clears the drag preview so the node snaps back to its
   * stored position instead of waiting for the 5 s fallback timer. */
  onNodeMoved?: (
    id: string,
    x: number,
    y: number,
  ) => undefined | boolean | Promise<undefined | boolean>;
  /** Called for **every** measure click, first included, with the snapped
   * position and what it snapped to (`null` for empty space). The parent owns
   * the point list — this component keeps no hidden anchor. */
  onMeasurePoint?: (
    position: [number, number],
    target: SnapResult["target"],
  ) => void;
  /** Committed measure points, in click order. Drives the rubber band. */
  measurePoints?: readonly [number, number][];
  /** Called when the user clicks empty canvas in add-node mode. */
  onCreateNodeRequest?: (lng: number, lat: number) => void;
  /** Called when the user selects two nodes in add-link mode. */
  onCreateLinkRequest?: (fromId: string, toId: string) => void;
  /** When flyToKey changes and flyToNodeId/flyToLinkId is set, the canvas animates to that element. */
  flyToNodeId?: string | null;
  flyToLinkId?: string | null;
  /** Fly to a region's extent (map mode; the schematic has no rings). */
  flyToRegionId?: string | null;
  flyToKey?: number;
  /** Increment to force the map/schematic to fit the full network extent.
   * Should change only on project switch (not scenario switch) so the user's
   * view position is preserved across scenario comparisons. */
  fitKey?: number;
  /**
   * Fired when the *user* moves the camera — a drag, a scroll-zoom, a
   * rotate — and never when the app moves it.
   *
   * The distinction is the whole point: it tells the caller whether the
   * current framing is the user's or the app's, and only a framing the app
   * chose may be replaced without asking.
   */
  onUserMovedViewport?: () => void;
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
  regions,
  viewMode,
  schematicScale = IDENTITY_SCALE,
  nodeScale = NODE_SCALE_DEFAULT,
  nodeVar,
  linkVar,
  animateLinks = true,
  periodResult = null,
  generic = null,
  basemap,
  basemapOpacity = 1,
  selectedNodeId,
  onSelectNode,
  selectedLinkId,
  onSelectLink,
  kindRoles,
  topological = false,
  couplingsResolved = true,
  couplings = EMPTY_COUPLINGS,
  selectedRegionId = null,
  onSelectRegion,
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
  pressureMin = 0,
  pressureMax = 100,
  velocityMax = 1.5,
  tool = "select",
  onNodeMoved,
  onCreateNodeRequest,
  onCreateLinkRequest,
  onMeasurePoint,
  measurePoints = EMPTY_MEASURE_POINTS,
  flyToNodeId,
  flyToLinkId,
  flyToRegionId,
  flyToKey,
  fitKey,
  onUserMovedViewport,
  zoomInKey,
  zoomOutKey,
  resetNorthKey,
  isActive = true,
}: MapCanvasProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapElRef = useRef<HTMLDivElement>(null);
  const deckHostRef = useRef<HTMLDivElement>(null);
  const { layers: canvasLayers } = useCanvasLayers();
  // Hover is shared state, not local: the network list and the inspector
  // drive it too, so hovering a row there lights the element up here.
  const { hoveredNodeId, hoveredLinkId, hoveredRegionId } = useHoverState();
  const {
    hoverNode: setHoveredNodeId,
    hoverLink: setHoveredLinkId,
    hoverRegion: setHoveredRegionId,
    clearHover,
  } = useHoverActions();
  // These refs deliberately track only what the *canvas* pointer is over,
  // never what the list or inspector hovers. They gate node dragging and
  // add-node placement, which must answer "what is under the cursor on the
  // map" — syncing them with the shared hover would make hovering a list row
  // silently refuse the next click on the map.
  const hoveredNodeIdRef = useRef<string | null>(null);
  const hoveredLinkIdRef = useRef<string | null>(null);
  // Cursor-following value chip: the element under the pointer + its position.
  const [hoverTip, setHoverTip] = useState<HoverTip | null>(null);
  const sys = useUnitSystem();
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
  const measureCursorRef = useRef<[number, number] | null>(null);
  /** What the cursor is over while measuring, for the measure-only highlight. */
  const measureHoverRef = useRef<SnapResult["target"]>(null);
  // Seeded lazily on first render: roughGeoViewState scans every node, so it
  // must not run as a useRef initializer argument (those are evaluated on
  // every render even though only the first value is kept).
  const viewStateLazyRef = useRef<CanvasViewState | null>(null);
  if (viewStateLazyRef.current === null) {
    viewStateLazyRef.current = roughGeoViewState(nodes);
  }
  const viewStateRef = viewStateLazyRef as { current: CanvasViewState };
  const prevViewModeRef = useRef<ViewMode | null>(null);
  // ── Basemap resolution ────────────────────────────────────────────────────
  // Provider ids resolve against the live catalog; before it arrives (or for
  // stale ids) resolution falls back to "streets" and self-corrects via the
  // setStyle effect once the catalog loads.
  const basemapProviders = useBasemapProviders();
  const resolvedBasemapStyle = useMemo(
    () => resolveBasemapStyle(basemap, basemapProviders),
    [basemap, basemapProviders],
  );
  // Latest resolved style, read by the map-creation effect via ref so the
  // effect does not depend on (and recreate the map for) style changes.
  const resolvedBasemapStyleRef = useRef(resolvedBasemapStyle);
  resolvedBasemapStyleRef.current = resolvedBasemapStyle;
  /** The style the MapLibre map currently has applied. */
  const appliedBasemapStyleRef = useRef<string | maplibregl.StyleSpecification>(
    resolvedBasemapStyle,
  );
  // Latest opacity, read by the map-creation effect via ref so re-mounts
  // re-apply it without the effect depending on the prop.
  const basemapOpacityRef = useRef(basemapOpacity);
  basemapOpacityRef.current = basemapOpacity;
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
  const regionsRef = useRef(regions);
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
    layout: SchematicLayout;
    /** Axis scales the cached coords were built at — moving either spacing
     * slider must invalidate them even though nodes/links are unchanged.
     * Compared by value, not object identity, so a caller that rebuilds the
     * scale object without changing it does not force a re-layout. */
    scaleX: number;
    scaleY: number;
    /** Couplings feed the connectivity pass, so a change in them must
     * invalidate the cached layout exactly as nodes and links do. */
    couplings: LayoutCoupling[];
    regions: Region[] | undefined;
  } | null>(null);
  const schematicLayout = useMemo(() => {
    // Checked before the cache, not after: the cache is keyed on the inputs
    // to the layout, and the mode is not one of them. Consulting it first
    // handed a plan view the schematic's positions — invisible for the
    // coordinates, which a plan view never reads, but the leader lines came
    // through and hung in space at schematic-space endpoints.
    if (!topological || !couplingsResolved) {
      // Drop a stale cache rather than pinning an obsolete full generation
      // of nodes/links/coords in memory until schematic is next opened.
      schematicCacheRef.current = null;
      return EMPTY_SCHEMATIC_LAYOUT;
    }
    const cache = schematicCacheRef.current;
    if (
      cache &&
      cache.nodes === nodes &&
      cache.links === links &&
      cache.couplings === couplings &&
      cache.regions === regions &&
      cache.scaleX === schematicScale.x &&
      cache.scaleY === schematicScale.y
    ) {
      return cache.layout;
    }
    const layout = computeSchematicLayout(
      nodes,
      links,
      schematicScale,
      couplings,
      regions ?? EMPTY_REGION_DATA,
    );
    schematicCacheRef.current = {
      nodes,
      links,
      layout,
      scaleX: schematicScale.x,
      scaleY: schematicScale.y,
      couplings,
      regions,
    };
    return layout;
  }, [
    nodes,
    links,
    topological,
    couplingsResolved,
    schematicScale,
    couplings,
    regions,
  ]);
  // Positions alone, for everything that only needs coordinates.
  const schematicCoords = schematicLayout.positions;

  /**
   * Resolve a measure click/hover at screen `(x, y)` to a point to snap to.
   *
   * Node before link before empty space: a node sitting on a link is the more
   * specific target, and its exact coordinates are what someone measuring
   * between two junctions wants. A link snaps to the nearest point along its
   * path rather than its midpoint — on a long main the midpoint can be a
   * kilometre from the click, which answers a different question.
   */
  const measureSnapAt = useCallback(
    (x: number, y: number, cursor: [number, number]): SnapResult => {
      const overlay = overlayRef.current;
      if (!overlay) return { position: cursor, target: null };
      const pick = (layerIds: string[]) => {
        try {
          return overlay.pickObject({
            x,
            y,
            radius: MEASURE_SNAP_RADIUS_PX,
            layerIds,
          });
        } catch {
          // A layer id that is not currently mounted (vertex-free networks omit
          // the "-path" variants) must not take the tool down with it.
          return null;
        }
      };
      const node = pick(["nodes"]);
      const nodeObj = node?.object as
        | { id: string; type: string; position: [number, number] }
        | undefined;
      if (nodeObj) {
        return {
          position: [nodeObj.position[0], nodeObj.position[1]],
          target: { kind: "node", id: nodeObj.id, type: nodeObj.type },
        };
      }
      const link = pick(["links-hittarget-path", "links-hittarget"]);
      const linkObj = link?.object as
        | { id: string; type: string; path?: [number, number][] }
        | undefined;
      if (linkObj?.path) {
        const snapped = nearestPointOnPath(linkObj.path, cursor);
        if (snapped) {
          return {
            position: snapped,
            target: { kind: "link", id: linkObj.id, type: linkObj.type },
          };
        }
      }
      return { position: cursor, target: null };
    },
    [],
  );

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

  // The positions actually on screen: invented by the layout in a
  // topological view, the model's own everywhere else. Every camera path in
  // the orthographic renderer must frame *these* — framing the layout's
  // positions in a plan view aimed the camera at an empty map, which is why
  // "Fit network" flew to nowhere.
  const renderCoords = topological ? schematicCoords : geoCoords;

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
      setHoverTip(null);
    }
    if (tool === "measure") {
      hoveredNodeIdRef.current = null;
      setHoveredNodeId(null);
      setHoverTip(null);
    }
  }, [tool, setHoveredNodeId, setHoveredLinkId]);
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
  useEffect(() => {
    regionsRef.current = regions;
  }, [regions]);

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
    measureCursorRef.current = null;
    measureHoverRef.current = null;
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

  const renderCoordsRef = useRef<Map<string, [number, number]>>(new Map());
  useEffect(() => {
    renderCoordsRef.current = renderCoords;
  }, [renderCoords]);

  // Fly/zoom to a specific element when flyToKey changes.
  useEffect(() => {
    if (!isActive) return;
    if (flyToKey == null) return;
    const nodeId = flyToNodeId;
    const linkId = flyToLinkId;
    const regionId = flyToRegionId;
    if (!nodeId && !linkId && !regionId) return;

    // Zooming to a region is still geographic-only. Both other views now
    // have a ring to frame — the model's own in a plan view, the placed
    // glyph in a schematic — but neither has the orthographic camera path
    // for it, so the request is dropped rather than aimed at nothing.
    if (regionId) {
      if (viewMode !== "map") return;
      const map = mapRef.current;
      const region = regionsRef.current?.find((r) => r.id === regionId);
      if (!map || !region || region.ring.length === 0) return;
      const first = region.ring[0];
      const bounds = region.ring.reduce(
        (b, p) => b.extend(p as [number, number]),
        new maplibregl.LngLatBounds(
          first as [number, number],
          first as [number, number],
        ),
      );
      map.fitBounds(bounds, {
        padding: visibleMapPadding(map),
        maxZoom: 18,
        duration: 800,
      });
      return;
    }

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
        map.flyTo({
          center,
          zoom,
          curve: 1,
          duration: 800,
          padding: visibleMapPadding(map),
        });
      } else if (linkId) {
        const link = linksRef.current.find((l) => l.id === linkId);
        if (!link) return;
        const from = geoCoordsRef.current.get(link.fromId);
        const to = geoCoordsRef.current.get(link.toId);
        if (!from || !to) return;
        const bounds = new maplibregl.LngLatBounds(from, from).extend(to);
        map.fitBounds(bounds, {
          padding: visibleMapPadding(map),
          maxZoom: 18,
          duration: 800,
        });
      }
    } else {
      // Schematic mode — orthographic view
      const deck = deckRef.current;
      if (!deck) return;
      const coords = renderCoordsRef.current;
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
  }, [flyToKey, isActive, viewMode, flyToLinkId, flyToNodeId, flyToRegionId]);

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
      // Positions come from the layout only where the layout invented them.
      const coordMap = renderCoords;
      // Display path precomputed once per network/viewMode change (not per
      // accessor call over 46k links). A topological layout ignores vertices
      // — it has no vertex positions, so links stay straight there.
      let anyLinkVertices = false;
      const linkData = links
        .map((l, si) => {
          const from = coordMap.get(l.fromId);
          const to = coordMap.get(l.toId);
          if (!from || !to) return null;
          const verts =
            !topological && l.vertices && l.vertices.length > 0
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
    }, [links, nodes, renderCoords, topological]);

  /**
   * The network's own spacing, for sizing nodes against it.
   *
   * Geographic view only. A schematic layout's distances are ours rather
   * than the model's — `computeSchematicLayout` builds them from a fixed
   * spacing multiplied by the aspect scale — so measuring them would make
   * node size a function of the spread slider, and that slider would stop
   * being only a spread slider. A fixed radius was always right there,
   * because the spacing it sits in is uniform and chosen.
   *
   * `null` therefore falls back to the constant the schematic has always
   * used, and the size slider still multiplies it.
   */
  const typicalLink = useMemo(
    () => (topological ? null : typicalLinkLength(linkData)),
    [linkData, topological],
  );

  // ── Viewport probe ─────────────────────────────────────────────────────────
  // The network list asks "is this element on screen"; only this component
  // knows both the viewport and the display coordinates. Answered per call
  // (~25 visible rows) rather than by precomputing a 46k-element set.
  const { setViewportProbe, viewportMoved } = useViewportActions();

  // Which renderer is on screen. `viewMode` still carries this — CanvasView
  // maps a local grid onto "schematic" because that is the orthographic
  // path — so it answers "is there a basemap", not "are the positions real".
  const orthographic = viewMode === "schematic";
  const orthographicRef = useRef(orthographic);
  orthographicRef.current = orthographic;

  /**
   * The visible extent, in whatever space the elements are drawn in.
   *
   * Two renderers, two sources: maplibre reports lng/lat bounds directly,
   * while the orthographic deck view states a centre and a log2 zoom, so its
   * extent is the canvas size divided by the scale that zoom implies.
   * `null` when neither is ready — the caller treats that as "visible", so a
   * canvas that has not laid out yet dims nothing.
   */
  const viewportBox = useCallback((): ViewportBox | null => {
    if (orthographicRef.current) {
      // deck's own viewport, rather than deriving the extent from the stored
      // camera and the canvas size by hand. It already knows the canvas
      // size, the zoom-to-scale convention and the view's Y orientation, and
      // it answers from whatever deck is *actually* rendering rather than
      // from a ref that several code paths write and one seeds with a
      // geographic camera. The hand-rolled version had a null to return for
      // each of those it could not satisfy, and a null reads as "everything
      // is visible".
      const viewport = deckRef.current?.getViewports()[0];
      if (!viewport) return null;
      const [west, south, east, north] = viewport.getBounds();
      return { west, south, east, north };
    }
    const map = mapRef.current;
    if (!map) return null;
    const b = map.getBounds();
    return {
      west: b.getWest(),
      south: b.getSouth(),
      east: b.getEast(),
      north: b.getNorth(),
    };
  }, []);
  const linkDatumByIdRef = useRef(linkDatumById);
  linkDatumByIdRef.current = linkDatumById;
  const viewportMovedRef = useRef(viewportMoved);
  viewportMovedRef.current = viewportMoved;

  // Throttled to ~10 Hz and shared by both renderers. Nothing in the list
  // reorders when the viewport moves, so rows fading as you pan is the
  // point — but maplibre's "move" and deck's onViewStateChange both fire per
  // frame, and a report per frame would re-render the list at 60 Hz.
  const viewportThrottleRef = useRef<number | null>(null);
  const reportViewportMoved = useCallback(() => {
    if (viewportThrottleRef.current != null) return;
    viewportThrottleRef.current = window.setTimeout(() => {
      viewportThrottleRef.current = null;
      viewportMovedRef.current();
    }, 100);
  }, []);
  /** Report immediately, cancelling any pending throttled report — for the
   * end of a gesture, so the settled viewport is never left showing the
   * state from 100 ms earlier. */
  const flushViewportMoved = useCallback(() => {
    if (viewportThrottleRef.current != null) {
      window.clearTimeout(viewportThrottleRef.current);
      viewportThrottleRef.current = null;
    }
    viewportMovedRef.current();
  }, []);
  useEffect(
    () => () => {
      if (viewportThrottleRef.current != null) {
        window.clearTimeout(viewportThrottleRef.current);
      }
    },
    [],
  );
  // Both renderers are wired up inside long-lived effects, so they reach
  // these through refs rather than re-registering their listeners.
  const reportViewportMovedRef = useRef(reportViewportMoved);
  reportViewportMovedRef.current = reportViewportMoved;
  const userMovedRef = useRef(onUserMovedViewport);
  userMovedRef.current = onUserMovedViewport;
  const flushViewportMovedRef = useRef(flushViewportMoved);
  flushViewportMovedRef.current = flushViewportMoved;

  useEffect(() => {
    // The question is "is this element on screen", which needs positions the
    // model actually stated. A topological layout invented them, so there is
    // nothing to compare and no probe is registered — that absence is what
    // hides the feature rather than offering a toggle that cannot answer.
    // A local grid is *not* excluded: its coordinates are perfectly real, it
    // simply draws them orthographically instead of on a basemap.
    if (topological) {
      setViewportProbe(null);
      return;
    }
    setViewportProbe((cls, id) => {
      const box = viewportBox();
      if (!box) return true;
      if (cls === "point") {
        const p = geoCoordsRef.current.get(id);
        return p ? pointInBox(p[0], p[1], box) : false;
      }
      if (cls === "polyline") {
        const datum = linkDatumByIdRef.current.get(id);
        return datum ? pathIntersectsBox(datum.path, box) : false;
      }
      const ring = regionsRef.current?.find((r) => r.id === id)?.ring;
      return ring ? ringIntersectsBox(ring, box) : false;
    });
    return () => setViewportProbe(null);
  }, [topological, setViewportProbe, viewportBox]);

  // Whether usable period results exist for the CURRENT topology. Guards
  // against a topology change racing ahead of the results that describe it —
  // the flat arrays are indexed by network position, so a length mismatch
  // would attach one element's values to another.
  //
  // Lifted to component scope because both the layer builder and the
  // flow-animation loop need it, and they must agree: a loop that ran while
  // the layers rendered neutral would burn frames animating nothing.
  const hasPeriodResults =
    periodResult != null &&
    periodResult.nodePressure.length === nodes.length &&
    periodResult.linkFlow.length === links.length;

  // Engine-generic channels, length-guarded at component scope for the same
  // topology-race reason as `hasPeriodResults`: a stale array must not
  // colour a changed network.
  const genNode =
    generic?.node?.values && generic.node.values.length === nodes.length
      ? generic.node
      : null;
  const genLink =
    generic?.link?.values && generic.link.values.length === links.length
      ? generic.link
      : null;
  const genRegion =
    generic?.region?.values &&
    generic.region.values.length === (regions?.length ?? 0)
      ? generic.region
      : null;

  const buildLayers = useCallback((): Layer[] => {
    const isSchematic = viewMode === "schematic";
    const coordSystem = isSchematic
      ? COORDINATE_SYSTEM.CARTESIAN
      : COORDINATE_SYSTEM.DEFAULT;

    // World-space units so sizes scale with zoom (metres on the geo map,
    // "common" units in the schematic). Per-layer pixel clamps bound them.
    const nodeRadiusUnits = isSchematic
      ? ("common" as const)
      : ("meters" as const);
    const linkWidthUnits = isSchematic
      ? ("common" as const)
      : ("meters" as const);

    // Sized against the network rather than fixed, so the node-to-link
    // ratio holds whether junctions are fifty metres or five kilometres
    // apart. See `nodeScale` for why zoom cannot substitute for this.
    //
    // Measured from the positions actually drawn, which in schematic view
    // are the laid-out ones rather than the model's own.
    const junctionRadius = nodeRadius(typicalLink, nodeScale);
    const specialRadius = junctionRadius * (9 / 7);

    // The clamps move with the slider, or it does nothing where it is most
    // wanted. A world radius is only visible between the floor and the
    // ceiling: zoomed out on a dense network every multiple of it is below
    // the floor, and zoomed in on a spread-out one every multiple is above
    // the ceiling. Scaling the bounds by the same factor keeps the control
    // effective at both ends while leaving the neutral position exactly
    // where the fixed values had it.
    const sizeFactor = nodeScaleFactor(nodeScale);
    const nodeMinPx = NODE_RADIUS_MIN_PX * sizeFactor;
    const nodeMaxPx = NODE_RADIUS_MAX_PX * sizeFactor;
    const nodeGlowMaxPx = NODE_GLOW_MAX_PX * sizeFactor;

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
    // Visibility is applied to the layer *data*, after the drag adjustment
    // above. Every link and node layer reads these two arrays, so one gate here
    // covers rendering, labels and picking together: an empty array draws
    // nothing and picks nothing, so a hidden element cannot be clicked. Gating
    // the layer list instead would mean threading a condition through a dozen
    // conditional spreads for no additional effect.
    if (!canvasLayers.links) ld = EMPTY_LINK_DATA as typeof ld;
    if (!canvasLayers.nodes) nd = EMPTY_NODE_DATA as typeof nd;

    const linkDatum = (id: string) =>
      drag ? ld.find((l) => l.id === id) : linkDatumById.get(id);
    const nodeDatum = (id: string) =>
      drag ? nd.find((n) => n.id === id) : nodeDatumById.get(id);

    // Period results are flat arrays in network order, looked up by each
    // datum's `si` (see `hasPeriodResults` for the topology guard).
    const pr = hasPeriodResults ? periodResult : null;
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

    // Shared colour accessors — used by BOTH the main node/link layers and
    // the hover/selection glow rings so halos always match the element.
    // Every node variable, and every link variable but Status, is derived
    // from results. Without them there is nothing to encode, so the network
    // renders neutral rather than showing a ramp colour for a value that
    // does not exist — the legend is hidden in this state too, so a colour
    // here would have nothing to explain it.
    const nodeColor = (d: (typeof nodeData)[number]): RGBA => {
      // Nothing to plot yet is not the same as a missing reading: the
      // network at rest gets its own palette, keyed on what each kind does.
      const role = kindRoles?.get(d.type);
      if (generic) {
        return genNode
          ? genericRgba(genNode.values?.[d.si], genNode.variable, 220, "point")
          : baseNodeRgba(role);
      }
      if (!pr) return baseNodeRgba(role);
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
        role,
        pressureMin,
        pressureMax,
      );
    };
    const linkColor = (d: (typeof linkData)[number]): RGBA => {
      const role = kindRoles?.get(d.type);
      if (generic) {
        return genLink
          ? genericRgba(
              genLink.values?.[d.si],
              genLink.variable,
              220,
              "polyline",
            )
          : baseLinkRgba(role);
      }
      // Status is the exception: it falls back to the model's initial
      // status, which is real data before any run.
      if (!pr && linkVar !== "status") return baseLinkRgba(role);
      return linkRgba(
        linkSim(d),
        linkVar,
        flowMax,
        velThresh,
        flowThresh,
        qualityMin,
        qualityMax,
        velocityMax,
      );
    };

    // Three concentric glow rings beneath a hovered/selected link. Returns []
    // when the id is null or has no drawable datum.
    const linkGlowLayers = (
      linkId: string | null,
      idPrefix: string,
      rings: readonly {
        suffix: string;
        alpha: number;
        width: number;
        rgb?: readonly [number, number, number];
      }[],
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
        widthUnits: linkWidthUnits,
        widthMaxPixels: LINK_GLOW_MAX_PX,
        capRounded: true as const,
        jointRounded: true as const,
        pickable: false as const,
        updateTriggers: {},
        data: [link],
      };
      return rings.map(
        ({ suffix, alpha, width, rgb }) =>
          new PathLayer({
            ...base,
            id: `${idPrefix}-${suffix}`,
            getColor: [
              rgb?.[0] ?? r,
              rgb?.[1] ?? g,
              rgb?.[2] ?? b,
              alpha,
            ] as unknown as RGBA,
            getWidth: width,
            // Per-ring pixel floor: each ring's nominal width doubles as its
            // minimum, so the halo keeps its full pad around the (clamped)
            // link at far zooms. Sharing the link's own floor here made the
            // rings converge with the link and selection became invisible.
            widthMinPixels: Math.max(LINK_WIDTH_MIN_PX, width),
          }),
      );
    };

    // Three concentric glow rings beneath a hovered/selected node.
    const nodeGlowLayers = (
      nodeId: string | null,
      idPrefix: string,
      rings: readonly {
        suffix: string;
        alpha: number;
        radiusPad: number;
        rgb?: readonly [number, number, number];
      }[],
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
        radiusMaxPixels: nodeGlowMaxPx,
        stroked: false,
        pickable: false as const,
        updateTriggers: {},
        data: [node],
      };
      return rings.map(
        ({ suffix, alpha, radiusPad, rgb }) =>
          new ScatterplotLayer({
            ...base,
            id: `${idPrefix}-${suffix}`,
            getRadius: baseR + radiusPad,
            getFillColor: [
              rgb?.[0] ?? r,
              rgb?.[1] ?? g,
              rgb?.[2] ?? b,
              alpha,
            ] as unknown as RGBA,
            // Per-ring pixel floor: keep the ring's pad visible around the
            // (clamped) node at far zooms. Sharing the node's own floor here
            // made the rings converge with the node and selection became
            // invisible when zoomed out.
            radiusMinPixels: nodeMinPx + radiusPad,
          }),
      );
    };

    const layers: Layer[] = [];

    // Subcatchment boundaries render beneath everything else: soft fills
    // with a hairline outline. In map mode the ring is the model's own plan
    // geometry; in the schematic it is the glyph the layout placed beside
    // the catchment's outlet, because the schematic has no plan to draw a
    // real ring on.
    const regionRing = (r: Region): Array<[number, number]> =>
      topological ? (schematicLayout.regionRings.get(r.id) ?? []) : r.ring;
    const placedRegions =
      canvasLayers.regions && regions
        ? regions.filter((r) => regionRing(r).length >= 3)
        : EMPTY_REGION_DATA;
    if (placedRegions.length > 0) {
      // With a generic region channel loaded, fill each polygon from its
      // value (regions and values share the snapshot order); otherwise the
      // neutral soft green. Kept translucent either way so the network
      // above stays readable. The selected ring brightens and thickens —
      // the areal equivalent of the node/link selection glow, which a
      // polygon cannot use (a halo around a catchment reads as a second
      // catchment).
      // Values are indexed by snapshot order, so a filtered draw list must
      // look its value up by id, never by its position in that list.
      const regionOrder = new Map(regions?.map((r, i) => [r.id, i]) ?? []);
      const baseFill: (r: Region) => RGBA = genRegion
        ? (r: Region) =>
            genericRgba(
              genRegion.values?.[regionOrder.get(r.id) ?? -1],
              genRegion.variable,
              110,
              "region",
            )
        : // At rest, before any result: neutral like every other class, so
          // the network reads by role rather than by hue. The green belongs
          // to catchment *data*, not to catchments as objects.
          () => [138, 147, 163, 30] as unknown as RGBA;
      const regionSelected = (r: Region) => r.id === selectedRegionId;
      // Hover brightens the ring short of the selected treatment, so the two
      // stay distinguishable when you hover something else while one region
      // is selected. A polygon cannot use the node/link halo — a glow around
      // a catchment reads as a second catchment.
      const regionHovered = (r: Region) =>
        r.id === hoveredRegionId && r.id !== selectedRegionId;
      // Regions sit beneath nodes and links, so deck's topmost-wins picking
      // still gives those priority; a click reaches a region only where no
      // element covers it.
      const regionsPickable = tool === "select" && onSelectRegion != null;
      layers.push(
        new PolygonLayer<Region>({
          id: "regions",
          data: placedRegions,
          getPolygon: regionRing,
          coordinateSystem: coordSystem,
          getFillColor: (r: Region) => {
            // Hover and selection lift the fill as well as the ring. Only
            // the alpha moves, and only a little: the hue is carrying a
            // result value, and a catchment that changes colour when the
            // pointer crosses it reads as a change in the data.
            const [red, green, blue, alpha] = baseFill(r) as unknown as [
              number,
              number,
              number,
              number,
            ];
            const lift = regionSelected(r) ? 40 : regionHovered(r) ? 22 : 0;
            return [
              red,
              green,
              blue,
              Math.min(255, alpha + lift),
            ] as unknown as RGBA;
          },
          getLineColor: (r: Region) =>
            (regionSelected(r)
              ? [226, 232, 242, 240]
              : regionHovered(r)
                ? [196, 205, 219, 205]
                : [138, 147, 163, 140]) as unknown as RGBA,
          getLineWidth: (r: Region) =>
            regionSelected(r) ? 2.5 : regionHovered(r) ? 1.8 : 1,
          lineWidthMinPixels: 1,
          lineWidthUnits: "pixels",
          stroked: true,
          filled: true,
          pickable: regionsPickable,
          onClick: (info: { object?: unknown }) => {
            if (toolRef.current !== "select") return;
            const obj = info.object as Region | undefined;
            if (obj) onSelectRegion?.(obj.id);
          },
          onHover: (info: { object?: unknown }) => {
            if (!regionsPickable) return;
            const obj = info.object as Region | undefined;
            setHoveredRegionId(obj?.id ?? null);
          },
          updateTriggers: {
            getPolygon: [topological, schematicLayout.regionRings],
            getFillColor: [
              genRegion?.values,
              genRegion?.variable,
              selectedRegionId,
              hoveredRegionId,
            ],
            getLineColor: [selectedRegionId, hoveredRegionId],
            getLineWidth: [selectedRegionId, hoveredRegionId],
          },
        }),
      );

      // Leader lines: in the schematic a glyph's position is a placement,
      // not a location, so the line to its outlet is what carries the
      // meaning. Dashed and dim, to read as annotation rather than a link.
      // Only a topological layout has leaders to draw: they explain a glyph
      // that was *placed* beside its outlet. Where the ring sits at its own
      // coordinates there is nothing to explain.
      const leaders = (topological ? placedRegions : EMPTY_REGION_DATA)
        .map((r) => schematicLayout.regionLeaders.get(r.id))
        .filter((l): l is [[number, number], [number, number]] => l != null);
      if (leaders.length > 0) {
        layers.push(
          new LineLayer<[[number, number], [number, number]]>({
            id: "region-leaders",
            data: leaders,
            coordinateSystem: coordSystem,
            getSourcePosition: (l) => l[0],
            getTargetPosition: (l) => l[1],
            getColor: [138, 147, 163, 95] as unknown as RGBA,
            getWidth: 1,
            widthUnits: "pixels",
            widthMinPixels: 1,
            pickable: false,
          }),
        );
      }
    }

    // Inlet couplings: the hydraulic path a dual-drainage model has where
    // no link exists. Drawn from the street conduit's midpoint to the node
    // it captures into, dashed and thin so it reads as "coupled to" rather
    // than as another pipe. Both endpoints must be placed for a connector
    // to be drawable, which the schematic guarantees and the map does not
    // (an element without coordinates is absent from the arrays).
    if (couplings.length > 0 && canvasLayers.links) {
      const linkMid = new Map<string, [number, number]>();
      for (const l of ld) {
        const path = l.path;
        if (path.length === 0) continue;
        // Midpoint of the drawn polyline, not of from→to: on a long
        // street the two differ enough to matter.
        const mid = path[Math.floor(path.length / 2)];
        linkMid.set(l.id, mid);
      }
      const nodePos = new Map<string, [number, number]>();
      for (const n of nd) nodePos.set(n.id, n.position);
      // Dashes are built geometrically rather than with deck's dash
      // extension: that lives in @deck.gl/extensions, and one dashed
      // connector does not justify the dependency. DASHES segments per
      // connector, drawn over the first 60% of each interval.
      const DASHES = 9;
      const couplingPaths: Array<[number, number][]> = [];
      for (const c of couplings) {
        const from = linkMid.get(c.link);
        const to = nodePos.get(c.node);
        if (!from || !to) continue;
        for (let i = 0; i < DASHES; i += 1) {
          const t0 = i / DASHES;
          const t1 = t0 + 0.6 / DASHES;
          couplingPaths.push([
            [
              from[0] + (to[0] - from[0]) * t0,
              from[1] + (to[1] - from[1]) * t0,
            ],
            [
              from[0] + (to[0] - from[0]) * t1,
              from[1] + (to[1] - from[1]) * t1,
            ],
          ]);
        }
      }
      if (couplingPaths.length > 0) {
        layers.push(
          new PathLayer({
            id: "inlet-couplings",
            data: couplingPaths,
            coordinateSystem: coordSystem,
            getPath: (d) => d,
            getColor: [138, 147, 163, 170] as unknown as RGBA,
            getWidth: 1.2,
            widthUnits: "pixels",
            pickable: false,
            updateTriggers: { getPath: [couplings] },
          }) as unknown as Layer,
        );
      }
    }

    // Detached group marker (schematic only). The layout parks anything not
    // reachable from a source in its own region to the right; without a boundary
    // and a label, that reads as a distant part of the network rather than as
    // "these are disconnected". Amber is free here: it is the warning colour and
    // the only other amber on the canvas belongs to the measure tool, which is
    // map-mode only, so the two can never appear together.
    if (topological && schematicLayout.detachedIds.size > 0) {
      let minX = Number.POSITIVE_INFINITY;
      let minY = Number.POSITIVE_INFINITY;
      let maxX = Number.NEGATIVE_INFINITY;
      let maxY = Number.NEGATIVE_INFINITY;
      for (const id of schematicLayout.detachedIds) {
        // Read through `schematicLayout` rather than the derived alias, so this
        // callback captures one value instead of two.
        const at = schematicLayout.positions.get(id);
        if (!at) continue;
        if (at[0] < minX) minX = at[0];
        if (at[0] > maxX) maxX = at[0];
        if (at[1] < minY) minY = at[1];
        if (at[1] > maxY) maxY = at[1];
      }
      if (Number.isFinite(minX)) {
        // Padding from the group's own extent, with a floor so a single
        // orphaned node still gets a box big enough to read as one.
        const pad = Math.max(
          DETACHED_BOX_MIN_PAD,
          Math.max(maxX - minX, maxY - minY) * DETACHED_BOX_PAD_FRACTION,
        );
        const x0 = minX - pad;
        const y0 = minY - pad;
        const x1 = maxX + pad;
        const y1 = maxY + pad;
        const count = schematicLayout.detachedIds.size;
        layers.push(
          new PathLayer({
            id: "detached-region",
            data: [
              [
                [x0, y0],
                [x1, y0],
                [x1, y1],
                [x0, y1],
                [x0, y0],
              ] as [number, number][],
            ],
            coordinateSystem: coordSystem,
            getPath: (d) => d,
            getColor: [212, 160, 23, 110] as unknown as RGBA,
            getWidth: 1.5,
            // Pixels, not world units: a hairline boundary should stay a
            // hairline at every zoom rather than thickening with the network.
            widthUnits: "pixels",
            pickable: false,
          }) as unknown as Layer,
          new TextLayer({
            id: "detached-region-label",
            data: [{ position: [x0, y0] as [number, number] }],
            coordinateSystem: coordSystem,
            getPosition: (d) => d.position,
            getText: () =>
              `Separate subnetwork · ${count} ${count === 1 ? "node" : "nodes"}`,
            getSize: 11,
            getColor: [212, 160, 23, 220] as unknown as RGBA,
            getTextAnchor: "start",
            getAlignmentBaseline: "bottom",
            getPixelOffset: [0, -6],
            background: false,
            fontFamily: "monospace",
            pickable: false,
            // Not subject to the node/link label cap: this is one string, and it
            // is the only thing that explains what the region is.
            updateTriggers: { getText: [count] },
          }) as unknown as Layer,
        );
      }
    }

    // Nodes and links are gated separately. A halo belongs to the element it
    // surrounds, so each kind's glows follow that kind's visibility — a
    // selection ring left floating where its link is hidden reads as a bug.
    const showNodes = canvasLayers.nodes;
    const showLinks = canvasLayers.links;
    if (showNodes || showLinks) {
      // ── Glow / halo layers — pushed FIRST so they render beneath links and nodes ──
      // Hover halos are suppressed while the same element is selected.
      layers.push(
        ...(showLinks
          ? linkGlowLayers(
              hoveredLinkId !== selectedLinkId ? hoveredLinkId : null,
              "hover-link-glow",
              LINK_HOVER_GLOW,
            )
          : []),
        ...(showLinks
          ? linkGlowLayers(
              selectedLinkId,
              "selection-link-glow",
              LINK_SELECTION_GLOW,
            )
          : []),
        ...nodeGlowLayers(
          showNodes && hoveredNodeId !== selectedNodeId ? hoveredNodeId : null,
          "hover-glow",
          NODE_HOVER_GLOW,
        ),
        ...nodeGlowLayers(
          showNodes ? selectedNodeId : null,
          "selection-glow",
          NODE_SELECTION_GLOW,
        ),
        // Measure snap preview. Routed through the same helpers so it scales
        // with zoom exactly as the hover and selection halos do — a bespoke
        // pixel-sized ring stayed the same size at every zoom and read as a
        // different kind of thing.
        ...nodeGlowLayers(
          showNodes &&
            tool === "measure" &&
            measureHoverRef.current?.kind === "node"
            ? measureHoverRef.current.id
            : null,
          "measure-glow",
          NODE_MEASURE_GLOW,
        ),
        ...linkGlowLayers(
          showLinks &&
            tool === "measure" &&
            measureHoverRef.current?.kind === "link"
            ? measureHoverRef.current.id
            : null,
          "measure-link-glow",
          LINK_MEASURE_GLOW,
        ),
      );

      // ── Links and nodes — rendered on top of all halos ──
      const onLinkHover = (info: {
        object?: unknown;
        x?: number;
        y?: number;
      }) => {
        // See the node layer's onHover: measure owns its own highlight.
        if (toolRef.current === "measure") return;
        const obj = info.object as
          | { id: string; si: number; type: string }
          | undefined;
        const id = obj ? obj.id : null;
        hoveredLinkIdRef.current = id;
        setHoveredLinkId(id);
        setHoverTip(
          obj && info.x != null && info.y != null
            ? {
                x: info.x,
                y: info.y,
                kind: "link",
                type: obj.type,
                si: obj.si,
                id: obj.id,
              }
            : null,
        );
      };
      const onLinkClick = (info: { object?: unknown }) => {
        // Measure consumes clicks in the map handler — see the node layer.
        if (toolRef.current === "measure") return;
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
        genLink?.values,
        genLink?.variable,
      ];
      // Link hover/click is only meaningful in select/edit; skipping the
      // pick pass for other tools halves per-mousemove GPU picking cost.
      // Measure snaps to links, so they must be pickable there too — the
      // handlers below no-op in measure mode and all of its interaction goes
      // through the map's own click/mousemove, which is where the snap radius
      // and the pick order (node before link) live.
      const linksPickable =
        tool === "select" || tool === "edit" || tool === "measure";
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
                widthUnits: linkWidthUnits,
                widthMinPixels: LINK_HIT_MIN_PX,
                widthMaxPixels: LINK_HIT_MAX_PX,
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
                widthUnits: linkWidthUnits,
                widthMinPixels: LINK_HIT_MIN_PX,
                widthMaxPixels: LINK_HIT_MAX_PX,
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
        ...(animateLinks && pr && pulseApplies(linkVar)
          ? [
              new FlowPathLayer({
                id: "links-flow",
                data: ld,
                coordinateSystem: coordSystem,
                // Geometry is static; flow direction is encoded in the sign
                // of the speed param so reverse flow never re-tesselates.
                getPath: (d) => d.path,
                getColor: linkColor,
                getWidth: 4,
                widthUnits: linkWidthUnits,
                widthMinPixels: LINK_WIDTH_MIN_PX,
                widthMaxPixels: LINK_WIDTH_MAX_PX,
                capRounded: true,
                jointRounded: true,
                pickable: false,
                flowTime: flowAnimRef.current,
                // What kind of claim the motion is making. See `linkPulse`.
                flowPattern: pulsePattern(pulseKindFor(linkVar)),
                // Speed and direction come from the link's own flow, not
                // from the coloured variable — what differs between
                // variables is only what motion is being asked to say. See
                // `linkPulse`.
                getFlowParams: (d) => [
                  pulseSpeed(pulseKindFor(linkVar), linkSim(d), flowMax),
                  hashStr(d.id) * 6.28318,
                ],
                updateTriggers: {
                  getColor: linkColorTriggers,
                  // `linkVar` belongs here: it decides the pulse kind, and a
                  // cached accessor would keep the previous variable's motion
                  // after a switch between two that both animate.
                  getFlowParams: [flowMax, pr, linkVar],
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
                  getWidth: 4,
                  widthUnits: linkWidthUnits,
                  widthMinPixels: LINK_WIDTH_MIN_PX,
                  widthMaxPixels: LINK_WIDTH_MAX_PX,
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
                  getWidth: 4,
                  widthUnits: linkWidthUnits,
                  widthMinPixels: LINK_WIDTH_MIN_PX,
                  widthMaxPixels: LINK_WIDTH_MAX_PX,
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
          radiusMinPixels: nodeMinPx,
          radiusMaxPixels: nodeMaxPx,
          // Pickable in measure mode too: snapping to a node needs it. The
          // hover/click handlers below bail out in measure mode.
          pickable: true,
          onHover: (info) => {
            // Measure draws its own highlight and its own readout; the normal
            // hover ring and value chip would compete with both.
            if (toolRef.current === "measure") return;
            const obj = info.object as
              | { id: string; si: number; type: string }
              | undefined;
            const id = obj ? obj.id : null;
            hoveredNodeIdRef.current = id;
            setHoveredNodeId(id);
            setHoverTip(
              obj && info.x != null && info.y != null
                ? {
                    x: info.x,
                    y: info.y,
                    kind: "node",
                    type: obj.type,
                    si: obj.si,
                    id: obj.id,
                  }
                : null,
            );
          },
          onClick: (info) => {
            if (didDragRef.current) {
              didDragRef.current = false;
              return;
            }
            if (toolRef.current === "edit") return;
            // Measure consumes clicks in the map handler, so that one place
            // decides the snap. Handling them here too would double-count.
            if (toolRef.current === "measure") return;
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
              genNode?.values,
              genNode?.variable,
            ],
            getRadius: nodeRadiusTrigger(isSchematic, typicalLink, nodeScale),
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

    // Measure overlay: committed points, the line between them (or to the
    // cursor while the second point is pending), and a highlight for whatever
    // the cursor is snapped to. All pixel-sized, so it reads the same at any
    // zoom.
    if (tool === "measure") {
      const committed = measurePoints;
      const rubberEnd =
        committed.length === 1 ? measureCursorRef.current : null;
      const segment =
        committed.length >= 2
          ? { from: committed[0], to: committed[1] }
          : rubberEnd
            ? { from: committed[0], to: rubberEnd }
            : null;
      if (segment) {
        layers.push(
          new LineLayer({
            id: "measure-line",
            data: [segment],
            coordinateSystem: coordSystem,
            getSourcePosition: (d) => d.from,
            getTargetPosition: (d) => d.to,
            getColor: [212, 160, 23, 200] as unknown as RGBA,
            getWidth: 2,
            widthUnits: "pixels",
            pickable: false,
          }) as unknown as Layer,
        );
      }
      const dots = rubberEnd ? [...committed, rubberEnd] : [...committed];
      if (dots.length > 0) {
        layers.push(
          new ScatterplotLayer({
            id: "measure-points",
            data: dots,
            coordinateSystem: coordSystem,
            getPosition: (d) => d,
            getRadius: 5,
            radiusUnits: "pixels",
            getFillColor: MEASURE_AMBER as unknown as RGBA,
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
    // `nodes`/`links` are no longer read here — the length guard they served
    // moved into `hasPeriodResults` (and the `gen*` channels), and
    // `nodeData`/`linkData` below already change whenever the network does.
    hasPeriodResults,
    generic,
    genNode,
    genLink,
    genRegion,
    viewMode,
    topological,
    kindRoles,
    regions,
    couplings,
    hoveredRegionId,
    setHoveredRegionId,
    setHoveredNodeId,
    setHoveredLinkId,
    selectedRegionId,
    onSelectRegion,
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
    pressureMin,
    pressureMax,
    velocityMax,
    measurePoints,
    schematicLayout,
    // Node radius is derived from these, so the layers must rebuild when
    // either the network's spacing or the slider changes.
    typicalLink,
    nodeScale,
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

  /**
   * The orthographic deck, created once.
   *
   * `initial` is the camera to be born with. The caller that creates the
   * deck has already worked out where to frame it, and deriving a second
   * answer here from a ref was how the deck came into existence pointing
   * somewhere else — painting one frame of the network at that camera
   * before the right one arrived. Only the first switch into the schematic
   * ever ran this, which is exactly when the flash appeared.
   */
  const ensureDeck = useCallback(
    (initial?: SchematicViewState) => {
      if (deckRef.current || !deckCanvasRef.current) return deckRef.current;
      const initialViewState =
        initial ?? orthoCenterFromMap(renderCoordsRef.current);
      viewStateRef.current = initialViewState;
      const deck = new Deck({
        // Our own canvas, not one deck makes for us. `deck.getCanvas()`
        // returns null immediately after construction — the device is set
        // up asynchronously — so every style this component applied to that
        // canvas was being skipped: the transparent background, the hide
        // before framing, the reveal after it, and the hide on returning to
        // the map. Supplying the element means it exists, is sized by CSS
        // and is styled by us before deck ever draws into it.
        canvas: deckCanvasRef.current,
        views: orthoViewRef.current,
        viewState: initialViewState,
        controller: true,
        pickingRadius: 6,
        onViewStateChange: ({
          viewState,
          interactionState,
        }: ViewStateChangeParameters<OrthographicViewState>) => {
          // Only a gesture counts. deck moves the camera itself when the
          // layout is framed, and that must not read as the user choosing
          // a view.
          if (
            interactionState?.isDragging ||
            interactionState?.isPanning ||
            interactionState?.isZooming ||
            interactionState?.isRotating
          ) {
            userMovedRef.current?.();
          }
          const nextViewState: SchematicViewState = {
            target: viewState.target as [number, number, number],
            zoom: Number(viewState.zoom ?? 0),
          };
          viewStateRef.current = nextViewState;
          deckRef.current?.setProps({ viewState: nextViewState });
          // Labels are viewport-culled; refresh them as the view moves.
          if (labelsOnRef.current) scheduleLabelRefresh("schematic");
          // Same report the map makes on "move": a plan view has a viewport
          // like any other, so the network list's dimming tracks it too.
          reportViewportMovedRef.current();
        },
        layers: [],
      });
      deckRef.current = deck;
      return deck;
    },
    [scheduleLabelRefresh],
  );

  useEffect(() => {
    if (!mapElRef.current) return;

    const initialVs = roughGeoViewState(nodesRef.current);
    appliedBasemapStyleRef.current = resolvedBasemapStyleRef.current;
    const map = new maplibregl.Map({
      container: mapElRef.current,
      // Read the style via the ref, NOT the `basemap` prop: having `basemap`
      // in this effect's deps tears down and recreates the whole map (losing
      // the viewport) on every style switch — the setStyle effect below
      // handles changes in place.
      style: resolvedBasemapStyleRef.current,
      center: [initialVs.longitude, initialVs.latitude],
      zoom: initialVs.zoom,
      attributionControl: false,
    });
    mapRef.current = map;

    // Correct the seed before anything paints.
    //
    // `initialVs` is a heuristic: it knows the network's extent but not the
    // container it has to fit inside, nor the rail, inspector, toolbar and
    // controls covering part of it. `cameraForBounds` knows all of that, and
    // works before the style loads because it is transform arithmetic. The
    // fit that used to happen on style.load therefore ran a frame or two
    // after the first paint, and you watched the map zoom out to it.
    if (nodesRef.current.length > 0) {
      fitMapExtents(nodesRef.current, map, {
        padding: visibleMapPadding(map),
      });
    }

    // Basemap dimming survives map re-creation: apply the current value to
    // the fresh canvas (see the basemapOpacity effect below for why the
    // canvas element and not the container).
    map.getCanvas().style.opacity = String(basemapOpacityRef.current);

    // Viewport reports for the network list's dimming. Throttled to ~10 Hz
    // during a drag: nothing reorders, so rows fading as you pan is the
    // point — but a report per frame would re-render the list at 60 Hz.
    map.on("move", () => reportViewportMovedRef.current());

    // `originalEvent` is present only when input drove the move; fitBounds
    // and easeTo arrive without one. That is the cleanest signal maplibre
    // offers for "the user did this".
    map.on("movestart", (e) => {
      if ((e as { originalEvent?: unknown }).originalEvent != null) {
        userMovedRef.current?.();
      }
    });

    map.on("moveend", () => {
      flushViewportMovedRef.current();
      if (labelsOnRef.current && viewModeRef.current === "map") {
        scheduleLabelRefresh("map");
      }
    });

    // Fires once for the initial style and again on every basemap switch.
    let firstStyleLoad = true;
    map.on("style.load", () => {
      const isInitialLoad = firstStyleLoad;
      firstStyleLoad = false;

      if (isInitialLoad) {
        // Initial load: reattach the overlay (harmless if already attached)
        // and fit the viewport to the network once the style is ready.
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
        fitMapExtents(nodesRef.current, map, {
          padding: visibleMapPadding(map),
        });
        return;
      }

      // Basemap switch: the deck overlay is non-interleaved — its canvas
      // lives outside the style, so it survives setStyle untouched. The old
      // remove/re-add dance here forced deck to rebuild and flash one frame
      // with a desynced (zoomed-in) viewport, and the refit below yanked the
      // user's pan/zoom back to the network extent. Just refresh the layers.
      if (isActiveRef.current && viewModeRef.current === "map") {
        overlayRef.current?.setProps({ layers: buildLayersRef.current() });
      }
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
        // The cursor drives both the rubber band and the snap preview, so the
        // highlight shows exactly where a click would land.
        const snapped = measureSnapAt(e.point.x, e.point.y, [lng, lat]);
        measureHoverRef.current = snapped.target;
        measureCursorRef.current = snapped.position;
        // A pointer cursor is the affordance that says "this click will snap".
        map.getCanvas().style.cursor = snapped.target ? "pointer" : "crosshair";
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
      const moveResult = onNodeMovedRef.current?.(
        nodeId,
        e.lngLat.lng,
        e.lngLat.lat,
      );
      // A handler that returns/resolves `false` declined the commit (e.g. the
      // drop point can't be converted to the source CRS) — snap the node back
      // to its stored position right away rather than leaving the preview
      // pinned until the fallback timer below fires.
      Promise.resolve(moveResult)
        // A rejected handler also means "not committed" (backend patch threw).
        .catch(() => false as const)
        .then((committed) => {
          if (
            committed === false &&
            !draggingNodeIdRef.current &&
            draggingNodePosRef.current?.id === nodeId
          ) {
            draggingNodePosRef.current = null;
            overlayRef.current?.setProps({ layers: buildLayersRef.current() });
          }
        });
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
        // Every click is reported, first included. The previous design kept the
        // first point in a ref here and never sent it, so the parent had one
        // point where it needed two: the first measurement showed nothing, and
        // every later one measured from the *previous* measurement's endpoint.
        const snapped = measureSnapAt(e.point.x, e.point.y, [lng, lat]);
        measureCursorRef.current = null;
        onMeasurePointRef.current?.(snapped.position, snapped.target);
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
  }, [markFirstFrame, measureSnapAt, scheduleLabelRefresh]);

  // Frames the network on arrival and when the network itself changes — and
  // otherwise leaves the camera exactly where the user put it.
  //
  // Reframing on every layout change would reset pan and zoom each time the
  // aspect slider moved, throwing away the view someone had set up to look at.
  // The reshape is visible without reframing, because the aspect slider holds
  // the two scales' product at 1: the layout's area is preserved, so it changes
  // proportions in place rather than growing off-screen.
  //
  // (Two independent per-axis sliders could not have had this. Their pair
  // carried a uniform component, which is only visible as a size change — so
  // holding the camera was the only way to see it, and reframing collapsed the
  // two tracks onto one degree of freedom.)
  const framedForRef = useRef<{ nodes: Node[]; links: Link[] } | null>(null);
  // One deck serves both orthographic views, but a plan and a schematic are
  // different coordinate spaces — the model's own units against the layout's
  // grid — so a camera carried from one to the other lands nowhere. Each
  // space keeps its own, restored on return and framed afresh the first time.
  const orthoSpaceRef = useRef<"plan" | "topological" | null>(null);
  const savedOrthoViewRef = useRef<
    Record<"plan" | "topological", SchematicViewState | null>
  >({ plan: null, topological: null });
  // The geographic camera gets the same treatment, so a trip to the
  // schematic and back does not cost you the place you were working.
  const savedMapViewRef = useRef<{
    center: [number, number];
    zoom: number;
    bearing: number;
    pitch: number;
  } | null>(null);
  useEffect(() => {
    // Leaving the orthographic renderer: keep the camera for the space it
    // belonged to, so coming back returns you to it rather than framing the
    // whole network again.
    if (viewMode !== "schematic") {
      const leaving = orthoSpaceRef.current;
      if (leaving) {
        savedOrthoViewRef.current[leaving] =
          viewStateRef.current as SchematicViewState;
      }
      return;
    }
    if (!isActive) return;
    // Nothing to frame yet: framing an empty layout would mark it framed and
    // the real one would never get its turn.
    if (topological && !couplingsResolved) return;
    const space = topological ? "topological" : "plan";
    const prevSpace = orthoSpaceRef.current;
    if (prevSpace && prevSpace !== space) {
      savedOrthoViewRef.current[prevSpace] =
        viewStateRef.current as SchematicViewState;
    }
    orthoSpaceRef.current = space;

    const framed = framedForRef.current;
    const networkChanged = framed?.nodes !== nodes || framed?.links !== links;
    // A camera saved against a different network frames the wrong thing.
    if (networkChanged) {
      savedOrthoViewRef.current = { plan: null, topological: null };
      savedMapViewRef.current = null;
    }
    framedForRef.current = { nodes, links };

    // Frame only when there is nothing to return to. A saved camera is the
    // record of having been here before, so it answers both "have I framed
    // this yet" and "where was I" — and fitting the network on every arrival
    // is what made switching views feel like it kept pressing Fit network.
    const saved = savedOrthoViewRef.current[space];
    const vs = saved ?? orthoCenterFromMap(renderCoords);

    // Framed before the deck exists, so the deck can be born looking at the
    // right place rather than corrected a frame later.
    const deck = ensureDeck(vs);
    if (!deck) return;
    viewStateRef.current = vs;
    deck.setProps({
      views: orthoViewRef.current,
      viewState: vs,
      layers: buildLayersRef.current(),
    });
    markFirstFrame("schematic");
    if (deckCanvasRef.current) {
      deckCanvasRef.current.style.display = "";
      deckCanvasRef.current.style.visibility = "visible";
    }
  }, [
    ensureDeck,
    isActive,
    links,
    markFirstFrame,
    nodes,
    renderCoords,
    topological,
    couplingsResolved,
    viewMode,
  ]);

  useEffect(() => {
    const deck = deckRef.current;
    if (!isActive || !deck || viewMode !== "schematic") return;
    deck.setProps({ layers: buildLayers(), viewState: viewStateRef.current });
    markFirstFrame("schematic");
  }, [buildLayers, isActive, markFirstFrame, viewMode]);

  // Mirrors the flow layer's own condition — with no results there is no
  // flow layer to drive, so the loop must not run either.
  const linkAnimationActive =
    animateLinks && hasPeriodResults && pulseApplies(linkVar);

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
  // Keyed on the *resolved* style (not the id): a provider id can re-resolve
  // when the catalog arrives, and a catalog refresh with unchanged content
  // must not restyle (sameBasemapStyle compares structurally).
  useEffect(() => {
    if (!isActive) return;
    const map = mapRef.current;
    if (!map) return;
    if (sameBasemapStyle(appliedBasemapStyleRef.current, resolvedBasemapStyle))
      return;
    appliedBasemapStyleRef.current = resolvedBasemapStyle;
    map.setStyle(resolvedBasemapStyle);
  }, [resolvedBasemapStyle, isActive]);

  // Basemap dimming — CSS opacity on the maplibre *canvas*, not the container
  // div: the non-interleaved MapboxOverlay renders the network to its own
  // sibling canvas inside the same container, so dimming the container would
  // dim network geometry too. maplibre keeps one canvas element across
  // setStyle, so the value survives basemap switches; the map-creation effect
  // re-applies it on re-mounts. (Deliberately NOT a style paint property —
  // CSS dims vector and provider-raster styles uniformly and live. With
  // basemap "none" this dims the blank background layer, which is harmless.)
  useEffect(() => {
    mapRef.current
      ?.getCanvas()
      .style.setProperty("opacity", String(basemapOpacity));
  }, [basemapOpacity]);

  // View mode switch.
  //
  // The map keeps its camera across a trip to the schematic, the way each
  // orthographic space keeps its own: someone who zoomed to a junction,
  // glanced at the topology and came back was looking at the whole network
  // again, having lost the place they were working. It is only framed on the
  // first arrival, or after a network change makes a saved camera frame the
  // wrong thing.
  useEffect(() => {
    if (!isActive) return;
    const enteringMapMode =
      viewMode === "map" && prevViewModeRef.current !== "map";
    const leavingMapMode =
      viewMode !== "map" && prevViewModeRef.current === "map";
    prevViewModeRef.current = viewMode;

    if (viewMode === "schematic") {
      if (leavingMapMode) {
        const map = mapRef.current;
        if (map) {
          const c = map.getCenter();
          savedMapViewRef.current = {
            center: [c.lng, c.lat],
            zoom: map.getZoom(),
            bearing: map.getBearing(),
            pitch: map.getPitch(),
          };
        }
      }
      // Clear overlay when entering schematic so no map-mode layer lingers.
      overlayRef.current?.setProps({ layers: [] });
      if (mapElRef.current) mapElRef.current.style.display = "none";
      // Revealing the deck is the framing effect's job, not this one's:
      // it may still be waiting for the couplings that decide the layout.
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
      if (map) {
        // Measure before framing. A project opened straight into the
        // schematic builds its map inside a `display: none` container, so
        // MapLibre's transform is sized to nothing — and it only watches the
        // *window* for resizes, never its own container. Every camera
        // computed against that transform came out at the wrong zoom, which
        // is why the first arrival in map view looked far out and every
        // arrival after it was right.
        map.resize();
        const saved = savedMapViewRef.current;
        if (saved) {
          map.jumpTo(saved);
        } else {
          fitMapExtents(nodesRef.current, map, {
            padding: visibleMapPadding(map),
          });
        }
      }
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
    // Same wait as the framing effect: there is no layout to fit to until
    // the couplings say which parts of the network are actually connected.
    if (topological && !couplingsResolved) return;

    if (viewMode === "schematic") {
      const deck = ensureDeck();
      if (!deck) return;
      const { target, zoom } = orthoCenterFromMap(renderCoords);
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
        fitMapExtents(nodes, map, { padding: visibleMapPadding(map) });
      } else {
        map.jumpTo({ center: [0, 20], zoom: 1 });
      }
    }
  }, [
    buildLayers,
    couplingsResolved,
    ensureDeck,
    fitKey,
    isActive,
    nodes,
    renderCoords,
    topological,
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
        // An explicit level rather than `auto`. The overlays above this are
        // composited layers — the inspector animates its opacity behind a
        // backdrop-filter — and a WebGL canvas whose stacking context is
        // only implied can be re-sorted above them while such a layer is
        // being rebuilt. Naming the level removes the ambiguity.
        zIndex: 0,
        cursor:
          hoveredNodeId != null || hoveredLinkId != null
            ? "pointer"
            : "default",
      }}
      onPointerLeave={() => {
        hoveredNodeIdRef.current = null;
        hoveredLinkIdRef.current = null;
        clearHover();
        setHoverTip(null);
      }}
    >
      <div ref={mapElRef} style={{ position: "absolute", inset: 0 }} />
      <div ref={deckHostRef} style={{ position: "absolute", inset: 0 }}>
        {/* Hidden with `visibility`, never `display`: a canvas that is not
            laid out measures zero, and deck would size its viewport from
            nothing. The framing effect makes it visible once it has a
            camera to show. */}
        <canvas
          ref={deckCanvasRef}
          style={{
            position: "absolute",
            inset: 0,
            width: "100%",
            height: "100%",
            zIndex: 1,
            background: "transparent",
            visibility: "hidden",
          }}
        />
      </div>
      <HoverChip
        tip={hoverTip}
        periodResult={periodResult}
        generic={generic}
        nodeVar={nodeVar}
        linkVar={linkVar}
        sys={sys}
      />
    </div>
  );
});
