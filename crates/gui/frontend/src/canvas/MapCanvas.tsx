import type { Deck as DeckType, Layer } from "@deck.gl/core";
import { COORDINATE_SYSTEM, Deck, OrthographicView } from "@deck.gl/core";
import {
  LineLayer,
  PathLayer,
  ScatterplotLayer,
  TextLayer,
} from "@deck.gl/layers";
import { MapboxOverlay } from "@deck.gl/mapbox";
import maplibregl from "maplibre-gl";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { Link, Node } from "../hooks";
import type { BasemapStyle } from "./Basemap";
import { FlowPathLayer } from "./FlowPathLayer";
import { useCanvasLayers } from "./layers-context";
import { hashStr, linkRgba, nodeRgba, type RGBA } from "./MapCanvas/colorUtils";
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
  layers: [{ id: "background", type: "background", paint: { "background-color": "#16181c" } }],
};

// "streets" = OpenFreeMap Liberty (full coloured streets)
// "light"   = OpenFreeMap Positron (minimal light theme)
// "dark"    = OpenFreeMap Dark (dark theme)
// "none"    = tile-free blank background
const MAP_STYLES: Record<BasemapStyle, string | maplibregl.StyleSpecification> = {
  streets: "https://tiles.openfreemap.org/styles/liberty",
  light: "https://tiles.openfreemap.org/styles/positron",
  dark: "https://tiles.openfreemap.org/styles/dark",
  none: BLANK_STYLE,
};

interface MapCanvasProps {
  nodes: Node[];
  links: Link[];
  viewMode: ViewMode;
  nodeVar: NodeVariable;
  linkVar: LinkVariable;
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
}

export function MapCanvas({
  nodes,
  links,
  viewMode,
  nodeVar,
  linkVar,
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
  const onSelectLinkRef = useRef(onSelectLink);
  const mapRef = useRef<maplibregl.Map | null>(null);
  const overlayRef = useRef<MapboxOverlay | null>(null);
  const deckRef = useRef<DeckType | null>(null);
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
  const viewStateRef = useRef<any>(roughGeoViewState(nodes));
  const prevViewModeRef = useRef<ViewMode | null>(null);
  const orthoViewRef = useRef(
    new OrthographicView({ id: "main", controller: true }),
  );
  const flowAnimRef = useRef(0);
  const buildLayersRef = useRef<() => Layer[]>(() => []);

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
  // Tracks whether the current mousedown actually moved — suppresses deck.gl onClick.
  const didDragRef = useRef(false);
  // In add-link mode: the ID of the first selected node, waiting for the second.
  const pendingLinkFromIdRef = useRef<string | null>(null);

  const schematicCoords = useMemo(
    () => computeSchematicLayout(nodes, links),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [nodes, links],
  );

  const geoCoords = useMemo(() => {
    const m = new Map<string, [number, number]>();
    for (const n of nodes) {
      if (n.x === 0 && n.y === 0) continue;
      m.set(n.id, [n.x, n.y]);
    }
    return m;
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
    onSelectLinkRef.current = onSelectLink;
  }, [onSelectLink]);
  useEffect(() => {
    toolRef.current = tool;
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
        const currentZoom = viewStateRef.current?.zoom ?? 12;
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
      if (nodeId) {
        const target = coords.get(nodeId);
        if (!target) return;
        const currentZoom = (viewStateRef.current as any)?.zoom ?? 0;
        const zoom = Math.max(currentZoom, currentZoom + 2);
        const vs = {
          target: [target[0], target[1], 0] as [number, number, number],
          zoom,
        };
        viewStateRef.current = vs;
        deck.setProps({ viewState: vs as any });
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
        const linkPx = Math.sqrt(
          (to[0] - from[0]) ** 2 + (to[1] - from[1]) ** 2,
        );
        const minDim = Math.min(viewW, viewH) * 0.4;
        // In orthographic mode zoom is pixels-per-unit, so zoom = minDim / linkPx.
        const currentZoom = (viewStateRef.current as any)?.zoom ?? 1;
        const zoom =
          linkPx > 0 ? Math.min(minDim / linkPx, currentZoom * 4) : currentZoom;
        const vs = { target: [cx, cy, 0] as [number, number, number], zoom };
        viewStateRef.current = vs;
        deck.setProps({ viewState: vs as any });
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [flyToKey, viewMode, flyToLinkId, flyToNodeId]);

  const buildLayers = useCallback((): Layer[] => {
    const isSchematic = viewMode === "schematic";
    const coordMap = isSchematic ? schematicCoords : geoCoords;
    const coordSystem = isSchematic
      ? COORDINATE_SYSTEM.CARTESIAN
      : COORDINATE_SYSTEM.DEFAULT;

    const nodeRadiusUnits = isSchematic
      ? ("common" as const)
      : ("pixels" as const);

    const junctionRadius = 7;
    const specialRadius = 9;

    const linkData = links
      .map((l) => {
        const drag = draggingNodePosRef.current;
        const dragPos: [number, number] | undefined = drag
          ? [drag.lng, drag.lat]
          : undefined;
        const from =
          (drag && drag.id === l.fromId ? dragPos : undefined) ??
          coordMap.get(l.fromId);
        const to =
          (drag && drag.id === l.toId ? dragPos : undefined) ??
          coordMap.get(l.toId);
        if (!from || !to) return null;
        return { ...l, from, to };
      })
      .filter(Boolean) as Array<
      Link & { from: [number, number]; to: [number, number] }
    >;

    const nodeData = nodes
      .map((n) => {
        // Override position while this node is being dragged.
        const drag = draggingNodePosRef.current;
        const position: [number, number] | undefined =
          drag && drag.id === n.id ? [drag.lng, drag.lat] : coordMap.get(n.id);
        if (!position) return null;
        return { ...n, position };
      })
      .filter(Boolean) as Array<Node & { position: [number, number] }>;

    const layers: Layer[] = [];

    if (canvasLayers.model) {
      // ── Glow / halo layers — pushed FIRST so they render beneath links and nodes ──
      layers.push(
        ...(() => {
          if (!hoveredLinkId || hoveredLinkId === selectedLinkId) return [];
          const hLink = linkData.find((l) => l.id === hoveredLinkId);
          if (!hLink) return [];
          const [r, g, b] = linkRgba(
            hLink,
            linkVar,
            flowMax,
            colorMode === "threshold" ? velocityThresholds : undefined,
            colorMode === "threshold" ? flowThresholds : undefined,
          );
          const base = {
            coordinateSystem: coordSystem,
            getPath: (d: typeof hLink) =>
              [d.from, d.to] as [[number, number], [number, number]],
            widthUnits: "pixels" as const,
            capRounded: true as const,
            jointRounded: true as const,
            pickable: false as const,
            updateTriggers: {},
            data: [hLink],
          };
          return [
            new PathLayer({
              ...base,
              id: "hover-link-glow-outer",
              getColor: [r, g, b, 20] as unknown as RGBA,
              getWidth: 18,
            }),
            new PathLayer({
              ...base,
              id: "hover-link-glow-mid",
              getColor: [r, g, b, 50] as unknown as RGBA,
              getWidth: 9,
            }),
            new PathLayer({
              ...base,
              id: "hover-link-glow-inner",
              getColor: [r, g, b, 90] as unknown as RGBA,
              getWidth: 4,
            }),
          ];
        })(),
        ...(() => {
          if (!selectedLinkId) return [];
          const sLink = linkData.find((l) => l.id === selectedLinkId);
          if (!sLink) return [];
          const [r, g, b] = linkRgba(
            sLink,
            linkVar,
            flowMax,
            colorMode === "threshold" ? velocityThresholds : undefined,
            colorMode === "threshold" ? flowThresholds : undefined,
          );
          const base = {
            coordinateSystem: coordSystem,
            getPath: (d: typeof sLink) =>
              [d.from, d.to] as [[number, number], [number, number]],
            widthUnits: "pixels" as const,
            capRounded: true as const,
            jointRounded: true as const,
            pickable: false as const,
            updateTriggers: {},
            data: [sLink],
          };
          return [
            new PathLayer({
              ...base,
              id: "selection-link-glow-outer",
              getColor: [r, g, b, 40] as unknown as RGBA,
              getWidth: 22,
            }),
            new PathLayer({
              ...base,
              id: "selection-link-glow-mid",
              getColor: [r, g, b, 90] as unknown as RGBA,
              getWidth: 10,
            }),
            new PathLayer({
              ...base,
              id: "selection-link-glow-inner",
              getColor: [r, g, b, 170] as unknown as RGBA,
              getWidth: 5,
            }),
          ];
        })(),
        ...(() => {
          if (!hoveredNodeId || hoveredNodeId === selectedNodeId) return [];
          const hNode = nodeData.find((n) => n.id === hoveredNodeId);
          if (!hNode) return [];
          const [r, g, b] = nodeRgba(
            hNode,
            nodeVar,
            headMin,
            headMax,
            demandMin,
            demandMax,
            qualityMin,
            qualityMax,
            colorMode === "threshold" ? pressureThresholds : undefined,
          );
          const baseR =
            hNode.type === "junction" ? junctionRadius : specialRadius;
          const base = {
            coordinateSystem: coordSystem,
            getPosition: (d: typeof hNode) => d.position,
            radiusUnits: nodeRadiusUnits,
            stroked: false,
            pickable: false as const,
            updateTriggers: {},
            data: [hNode],
          };
          return [
            new ScatterplotLayer({
              ...base,
              id: "hover-glow-outer",
              getRadius: baseR + 14,
              getFillColor: [r, g, b, 18] as unknown as RGBA,
            }),
            new ScatterplotLayer({
              ...base,
              id: "hover-glow-mid",
              getRadius: baseR + 8,
              getFillColor: [r, g, b, 40] as unknown as RGBA,
            }),
            new ScatterplotLayer({
              ...base,
              id: "hover-glow-inner",
              getRadius: baseR + 4,
              getFillColor: [r, g, b, 70] as unknown as RGBA,
            }),
          ];
        })(),
        ...(() => {
          if (!selectedNodeId) return [];
          const sNode = nodeData.find((n) => n.id === selectedNodeId);
          if (!sNode) return [];
          const [r, g, b] = nodeRgba(
            sNode,
            nodeVar,
            headMin,
            headMax,
            demandMin,
            demandMax,
            qualityMin,
            qualityMax,
            colorMode === "threshold" ? pressureThresholds : undefined,
          );
          const baseR =
            sNode.type === "junction" ? junctionRadius : specialRadius;
          const base = {
            coordinateSystem: coordSystem,
            getPosition: (d: typeof sNode) => d.position,
            radiusUnits: nodeRadiusUnits,
            stroked: false,
            pickable: false as const,
            updateTriggers: {},
            data: [sNode],
          };
          return [
            new ScatterplotLayer({
              ...base,
              id: "selection-glow-outer",
              getRadius: baseR + 18,
              getFillColor: [r, g, b, 35] as unknown as RGBA,
            }),
            new ScatterplotLayer({
              ...base,
              id: "selection-glow-mid",
              getRadius: baseR + 11,
              getFillColor: [r, g, b, 80] as unknown as RGBA,
            }),
            new ScatterplotLayer({
              ...base,
              id: "selection-glow-inner",
              getRadius: baseR + 5,
              getFillColor: [r, g, b, 140] as unknown as RGBA,
            }),
          ];
        })(),
      );

      // ── Links and nodes — rendered on top of all halos ──
      layers.push(
        new LineLayer({
          id: "links-hittarget",
          data: linkData,
          coordinateSystem: coordSystem,
          getSourcePosition: (d) => d.from,
          getTargetPosition: (d) => d.to,
          getColor: [0, 0, 0, 0] as unknown as RGBA,
          getWidth: 12,
          widthUnits: "pixels" as const,
          pickable: true,
          onHover: (info) => {
            const id = info.object ? (info.object as { id: string }).id : null;
            hoveredLinkIdRef.current = id;
            setHoveredLinkId(id);
          },
          onClick: (info) => {
            if (info.object) {
              const id = (info.object as { id: string }).id;
              onSelectLink(id === selectedLinkId ? null : id);
            }
          },
          updateTriggers: {},
        }),
        ...(linkVar === "flow"
          ? [
              new FlowPathLayer({
                id: "links",
                data: linkData,
                coordinateSystem: coordSystem,
                getPath: (d) =>
                  (d.flow != null && d.flow < 0
                    ? [d.to, d.from]
                    : [d.from, d.to]) as [number, number][],
                getColor: (d) =>
                  linkRgba(
                    d,
                    linkVar,
                    flowMax,
                    undefined,
                    colorMode === "threshold" ? flowThresholds : undefined,
                  ),
                getWidth: 2,
                widthUnits: "pixels" as const,
                rounded: true,
                capRounded: true,
                jointRounded: true,
                pickable: false,
                getFlowTime: () => flowAnimRef.current,
                getFlowSpeed: (d) => {
                  const v = (d as { velocity?: number }).velocity;
                  if (v != null && v > 0) return Math.min(1, v / 1.5);
                  const f = (d as { flow?: number | null }).flow;
                  return f != null
                    ? Math.min(1, Math.abs(f) / Math.max(0.01, flowMax))
                    : 0.2;
                },
                getFlowFrequency: (_d: unknown) => 1.0,
                getFlowPhaseOffset: (d: { id: string }) =>
                  hashStr(d.id) * 6.28318,
                updateTriggers: {
                  getColor: [linkVar, flowMax, colorMode, flowThresholds],
                  getFlowTime: [flowAnimRef.current],
                  getFlowSpeed: [flowMax],
                },
              }),
            ]
          : [
              new LineLayer({
                id: "links",
                data: linkData,
                coordinateSystem: coordSystem,
                getSourcePosition: (d) => d.from,
                getTargetPosition: (d) => d.to,
                getColor: (d) =>
                  linkRgba(
                    d,
                    linkVar,
                    flowMax,
                    colorMode === "threshold" ? velocityThresholds : undefined,
                    undefined,
                  ),
                getWidth: 2,
                widthUnits: "pixels" as const,
                pickable: false,
                updateTriggers: {
                  getColor: [linkVar, flowMax, colorMode, velocityThresholds],
                },
              }),
            ]),
        new ScatterplotLayer({
          id: "nodes",
          data: nodeData,
          coordinateSystem: coordSystem,
          getPosition: (d) => d.position,
          getFillColor: (d) =>
            nodeRgba(
              d,
              nodeVar,
              headMin,
              headMax,
              demandMin,
              demandMax,
              qualityMin,
              qualityMax,
              colorMode === "threshold" ? pressureThresholds : undefined,
            ),
          getRadius: (d) =>
            d.type === "junction" ? junctionRadius : specialRadius,
          radiusUnits: nodeRadiusUnits,
          pickable: true,
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
            ],
            getRadius: [isSchematic],
          },
        }),
      );
    }

    if (canvasLayers.nodeLabels) {
      layers.push(
        new TextLayer({
          id: "labels-nodes",
          data: nodeData,
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
      layers.push(
        new TextLayer({
          id: "labels-links",
          data: linkData,
          coordinateSystem: coordSystem,
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
    nodes,
    links,
    schematicCoords,
    geoCoords,
    viewMode,
    nodeVar,
    linkVar,
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
    colorMode,
    pressureThresholds,
    velocityThresholds,
    flowThresholds,
  ]);

  useEffect(() => {
    buildLayersRef.current = buildLayers;
  }, [buildLayers]);

  // Clear the drag-position override once geoCoords has been rebuilt with the
  // updated coordinates from the backend.  Keying on geoCoords (not nodes)
  // ensures the new coordMap is in place before buildLayers uses it.
  useEffect(() => {
    draggingNodePosRef.current = null;
  }, []);

  const ensureDeck = useCallback(() => {
    if (deckRef.current || !deckHostRef.current) return deckRef.current;
    const deck = new Deck({
      parent: deckHostRef.current,
      style: { position: "absolute", inset: "0", zIndex: "1" },
      views: orthoViewRef.current as any,
      viewState: viewStateRef.current,
      controller: true,
      pickingRadius: 6,
      onViewStateChange: ({ viewState }: any) => {
        viewStateRef.current = viewState;
        deckRef.current?.setProps({ viewState });
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
  }, [viewMode]);

  useEffect(() => {
    if (!mapElRef.current) return;

    const initialVs = roughGeoViewState(nodesRef.current);
    const map = new maplibregl.Map({
      container: mapElRef.current,
      style: MAP_STYLES[basemap],
      center: [initialVs.longitude, initialVs.latitude],
      zoom: initialVs.zoom,
      attributionControl: false,
    });
    mapRef.current = map;

    map.on("style.load", () => {
      fitMapExtents(nodesRef.current, map);
    });

    const overlay = new MapboxOverlay({ layers: [], pickingRadius: 6 });
    map.addControl(overlay as any);
    overlayRef.current = overlay;

    map.on("mousedown", (e) => {
      if (toolRef.current !== "edit") return;
      const nodeId = hoveredNodeIdRef.current;
      if (!nodeId) return;
      didDragRef.current = false;
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
    });
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
      try {
        map.removeControl(overlay as any);
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
      overlayRef.current = null;
      deckRef.current = null;
      deckCanvasRef.current = null;
      mapRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [basemap]);

  useEffect(() => {
    if (viewMode !== "schematic") return;
    const deck = ensureDeck();
    if (!deck) return;
    const { target, zoom } = orthoCenterFromMap(schematicCoords);
    const vs = { target, zoom };
    viewStateRef.current = vs;
    deck.setProps({
      views: orthoViewRef.current as any,
      viewState: vs as any,
      layers: buildLayersRef.current(),
    });
    if (deckCanvasRef.current) deckCanvasRef.current.style.display = "";
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [ensureDeck, schematicCoords, viewMode]);

  useEffect(() => {
    const deck = deckRef.current;
    if (!deck || viewMode !== "schematic") return;
    deck.setProps({ layers: buildLayers(), viewState: viewStateRef.current });
  }, [buildLayers, viewMode]);

  useEffect(() => {
    if (viewMode !== "schematic" || linkVar !== "flow") {
      flowAnimRef.current = 0;
      return;
    }
    let rafId: number;
    let lastTs = performance.now();
    function tick(now: number) {
      const dt = Math.min(now - lastTs, 50);
      lastTs = now;
      flowAnimRef.current += dt * 0.001;
      deckRef.current?.setProps({ layers: buildLayersRef.current() });
      rafId = requestAnimationFrame(tick);
    }
    rafId = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafId);
  }, [linkVar, viewMode]);

  // Update overlay when data/layers change in map mode.
  useEffect(() => {
    if (viewMode !== "map") return;
    overlayRef.current?.setProps({ layers: buildLayers() });
  }, [buildLayers, viewMode]);

  // Map-mode flow animation via overlay.
  useEffect(() => {
    if (viewMode !== "map" || linkVar !== "flow") return;
    let rafId: number;
    let lastTs = performance.now();
    function tick(now: number) {
      const dt = Math.min(now - lastTs, 50);
      lastTs = now;
      flowAnimRef.current += dt * 0.001;
      overlayRef.current?.setProps({ layers: buildLayersRef.current() });
      rafId = requestAnimationFrame(tick);
    }
    rafId = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(rafId);
  }, [linkVar, viewMode]);

  // Basemap style change — MapboxOverlay re-attaches automatically as IControl.
  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;
    map.setStyle(MAP_STYLES[basemap]);
  }, [basemap]);

  // View mode switch.
  useEffect(() => {
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
  }, [viewMode]);

  // ── Fit-to-network: fires when nodes first arrive (initial load) or when
  //    fitKey changes (explicit project switch).  Does NOT fire on scenario
  //    switches so the user's chosen view position is preserved.
  const prevHasNodesRef = useRef(nodes.length > 0);
  const prevFitKeyRef = useRef(fitKey);
  useEffect(() => {
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
        views: orthoViewRef.current as any,
        viewState: vs as any,
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
  }, [buildLayers, ensureDeck, fitKey, nodes, schematicCoords, viewMode]);

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
}
