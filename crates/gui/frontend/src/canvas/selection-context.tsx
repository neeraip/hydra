/**
 * Canvas selection context.
 *
 * Allows `CanvasView` (which hosts the canvas) and `CanvasRail` (inside
 * `SecondaryRail`, a sibling in the App tree) to share selection state and
 * the floating inspector view without prop-drilling through `App.tsx`.
 */

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
} from "react";
import type { GenericQuantity } from "../hooks/results";
import type { Link, Node, Region } from "../types/network";

export type InspectorView = "closed" | "node" | "link" | "region";

/** Header of one engine-generic result column in the rail list: the
 * engine-authored label/unit of a catalog variable whose per-element
 * values CanvasView merged into the sim arrays (`resultValues[key]`). */
export interface SimResultColumn {
  /** Variable id — the key into each element's `resultValues`. */
  key: string;
  label: string;
  /** Engine-authored compact notation for the header's narrowest stage. */
  symbol?: string;
  /** §5 quantity descriptor for the column's SI values. */
  quantity?: GenericQuantity;
}

/** Per-class result columns, `null` when the engine serves fixed-variable
 * results (wds) — the rail then keeps its built-in columns. */
export interface SimResultColumns {
  node: SimResultColumn[];
  link: SimResultColumn[];
  region: SimResultColumn[];
}

interface CanvasSelectionCtx {
  selectedNodeId: string | null;
  selectedLinkId: string | null;
  /** Selected areal element (subcatchment), for engines that have them. */
  selectedRegionId: string | null;
  inspectorView: InspectorView;
  /** Smart select: handles toggle-off when the same id is passed again. */
  selectNode: (id: string | null) => void;
  /** Smart select: handles toggle-off when the same id is passed again. */
  selectLink: (id: string | null) => void;
  /** Smart select: handles toggle-off when the same id is passed again. */
  selectRegion: (id: string | null) => void;
  /** Raw inspector view setter for cases that need explicit control. */
  setInspectorView: (v: InspectorView) => void;
  /** Raw node id setter — use when selection state needs updating without toggle logic. */
  setSelectedNodeId: (id: string | null) => void;
  /** Raw link id setter — use when selection state needs updating without toggle logic. */
  setSelectedLinkId: (id: string | null) => void;
  /** Raw region id setter — see `setSelectedNodeId`. */
  setSelectedRegionId: (id: string | null) => void;
  /** Clears both selection ids and closes the inspector in one call. */
  clearSelection: () => void;
  /** Simulation-merged node/link arrays written by CanvasView so the rail
   *  can display live result values without re-fetching from the backend. */
  simNodes: Node[] | null;
  simLinks: Link[] | null;
  simRegions: Region[] | null;
  /** Generic result-column headers accompanying the arrays (engines whose
   * values ride on `resultValue`); `null` for wds. */
  simColumns: SimResultColumns | null;
  setSimData: (
    nodes: Node[],
    links: Link[],
    regions: Region[],
    columns?: SimResultColumns | null,
  ) => void;
  /** Animate the canvas to a specific node. No-op when no canvas is mounted. */
  zoomToNode: (id: string) => void;
  /** Animate the canvas to a specific link. No-op when no canvas is mounted. */
  zoomToLink: (id: string) => void;
  /** Animate the canvas to a specific region. No-op when no canvas is mounted. */
  zoomToRegion: (id: string) => void;
  /** Called by CanvasView on mount to register the fly-to callbacks. */
  setZoomCallbacks: (
    nodeZoom: (id: string) => void,
    linkZoom: (id: string) => void,
    regionZoom: (id: string) => void,
  ) => void;
}

const Ctx = createContext<CanvasSelectionCtx>({
  selectedNodeId: null,
  selectedLinkId: null,
  selectedRegionId: null,
  inspectorView: "closed",
  selectNode: () => {},
  selectLink: () => {},
  selectRegion: () => {},
  setInspectorView: () => {},
  setSelectedNodeId: () => {},
  setSelectedLinkId: () => {},
  setSelectedRegionId: () => {},
  clearSelection: () => {},
  simNodes: null,
  simLinks: null,
  simRegions: null,
  simColumns: null,
  setSimData: () => {},
  zoomToNode: () => {},
  zoomToLink: () => {},
  zoomToRegion: () => {},
  setZoomCallbacks: () => {},
});

export function CanvasSelectionProvider({ children }: { children: ReactNode }) {
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedLinkId, setSelectedLinkId] = useState<string | null>(null);
  const [selectedRegionId, setSelectedRegionId] = useState<string | null>(null);
  const [inspectorView, setInspectorView] = useState<InspectorView>("closed");
  const [simNodes, setSimNodes] = useState<Node[] | null>(null);
  const [simLinks, setSimLinks] = useState<Link[] | null>(null);
  const [simRegions, setSimRegions] = useState<Region[] | null>(null);
  const [simColumns, setSimColumns] = useState<SimResultColumns | null>(null);

  const setSimData = useCallback(
    (
      nodes: Node[],
      links: Link[],
      regions: Region[],
      columns?: SimResultColumns | null,
    ) => {
      setSimNodes(nodes);
      setSimLinks(links);
      setSimRegions(regions);
      setSimColumns(columns ?? null);
    },
    [],
  );

  // Ref-based zoom callbacks so CanvasView can register them without causing
  // re-renders on every flyToState change.
  const zoomToNodeRef = useRef<(id: string) => void>(() => {});
  const zoomToLinkRef = useRef<(id: string) => void>(() => {});
  const zoomToRegionRef = useRef<(id: string) => void>(() => {});
  const zoomToNode = useCallback((id: string) => zoomToNodeRef.current(id), []);
  const zoomToLink = useCallback((id: string) => zoomToLinkRef.current(id), []);
  const zoomToRegion = useCallback(
    (id: string) => zoomToRegionRef.current(id),
    [],
  );
  const setZoomCallbacks = useCallback(
    (
      nodeZoom: (id: string) => void,
      linkZoom: (id: string) => void,
      regionZoom: (id: string) => void,
    ) => {
      zoomToNodeRef.current = nodeZoom;
      zoomToLinkRef.current = linkZoom;
      zoomToRegionRef.current = regionZoom;
    },
    [],
  );

  // Stable refs so callbacks don't go stale when selection changes.
  const nodeIdRef = useRef<string | null>(null);
  const linkIdRef = useRef<string | null>(null);
  const regionIdRef = useRef<string | null>(null);
  nodeIdRef.current = selectedNodeId;
  linkIdRef.current = selectedLinkId;
  regionIdRef.current = selectedRegionId;

  const selectNode = useCallback((id: string | null) => {
    if (!id) {
      setSelectedNodeId(null);
      setInspectorView("closed");
      return;
    }
    if (nodeIdRef.current === id) {
      // Tap same node again → deselect and close.
      setSelectedNodeId(null);
      setInspectorView("closed");
      return;
    }
    setSelectedNodeId(id);
    setSelectedLinkId(null);
    setSelectedRegionId(null);
    setInspectorView("node");
  }, []);

  const selectLink = useCallback((id: string | null) => {
    if (!id) {
      setSelectedLinkId(null);
      setInspectorView("closed");
      return;
    }
    if (linkIdRef.current === id) {
      // Tap same link again → deselect and close.
      setSelectedLinkId(null);
      setInspectorView("closed");
      return;
    }
    setSelectedLinkId(id);
    setSelectedNodeId(null);
    setSelectedRegionId(null);
    setInspectorView("link");
  }, []);

  const selectRegion = useCallback((id: string | null) => {
    if (!id || regionIdRef.current === id) {
      // Tap the same region again → deselect and close.
      setSelectedRegionId(null);
      setInspectorView("closed");
      return;
    }
    setSelectedRegionId(id);
    setSelectedNodeId(null);
    setSelectedLinkId(null);
    setInspectorView("region");
  }, []);

  const clearSelection = useCallback(() => {
    setSelectedNodeId(null);
    setSelectedLinkId(null);
    setSelectedRegionId(null);
    setInspectorView("closed");
  }, []);

  // Memoized so provider-parent renders don't hand every consumer a fresh
  // context value (the sim arrays alone make consumer re-renders expensive).
  const value = useMemo(
    () => ({
      selectedNodeId,
      selectedLinkId,
      selectedRegionId,
      inspectorView,
      selectNode,
      selectLink,
      selectRegion,
      setInspectorView,
      setSelectedNodeId,
      setSelectedLinkId,
      setSelectedRegionId,
      clearSelection,
      simNodes,
      simLinks,
      simRegions,
      simColumns,
      setSimData,
      zoomToNode,
      zoomToLink,
      zoomToRegion,
      setZoomCallbacks,
    }),
    [
      selectedNodeId,
      selectedLinkId,
      selectedRegionId,
      inspectorView,
      selectNode,
      selectLink,
      selectRegion,
      clearSelection,
      simNodes,
      simLinks,
      simRegions,
      simColumns,
      setSimData,
      zoomToNode,
      zoomToLink,
      zoomToRegion,
      setZoomCallbacks,
    ],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useCanvasSelection() {
  return useContext(Ctx);
}
