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
import type { Link, Node } from "../types/network";

export type InspectorView = "closed" | "node" | "link";

interface CanvasSelectionCtx {
  selectedNodeId: string | null;
  selectedLinkId: string | null;
  inspectorView: InspectorView;
  /** Smart select: handles toggle-off when the same id is passed again. */
  selectNode: (id: string | null) => void;
  /** Smart select: handles toggle-off when the same id is passed again. */
  selectLink: (id: string | null) => void;
  /** Raw inspector view setter for cases that need explicit control. */
  setInspectorView: (v: InspectorView) => void;
  /** Raw node id setter — use when selection state needs updating without toggle logic. */
  setSelectedNodeId: (id: string | null) => void;
  /** Raw link id setter — use when selection state needs updating without toggle logic. */
  setSelectedLinkId: (id: string | null) => void;
  /** Clears both selection ids and closes the inspector in one call. */
  clearSelection: () => void;
  /** Simulation-merged node/link arrays written by CanvasView so the rail
   *  can display live result values without re-fetching from the backend. */
  simNodes: Node[] | null;
  simLinks: Link[] | null;
  setSimData: (nodes: Node[], links: Link[]) => void;
  /** Animate the canvas to a specific node. No-op when no canvas is mounted. */
  zoomToNode: (id: string) => void;
  /** Animate the canvas to a specific link. No-op when no canvas is mounted. */
  zoomToLink: (id: string) => void;
  /** Called by CanvasView on mount to register the fly-to callbacks. */
  setZoomCallbacks: (
    nodeZoom: (id: string) => void,
    linkZoom: (id: string) => void,
  ) => void;
}

const Ctx = createContext<CanvasSelectionCtx>({
  selectedNodeId: null,
  selectedLinkId: null,
  inspectorView: "closed",
  selectNode: () => {},
  selectLink: () => {},
  setInspectorView: () => {},
  setSelectedNodeId: () => {},
  setSelectedLinkId: () => {},
  clearSelection: () => {},
  simNodes: null,
  simLinks: null,
  setSimData: () => {},
  zoomToNode: () => {},
  zoomToLink: () => {},
  setZoomCallbacks: () => {},
});

export function CanvasSelectionProvider({ children }: { children: ReactNode }) {
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [selectedLinkId, setSelectedLinkId] = useState<string | null>(null);
  const [inspectorView, setInspectorView] = useState<InspectorView>("closed");
  const [simNodes, setSimNodes] = useState<Node[] | null>(null);
  const [simLinks, setSimLinks] = useState<Link[] | null>(null);

  const setSimData = useCallback((nodes: Node[], links: Link[]) => {
    setSimNodes(nodes);
    setSimLinks(links);
  }, []);

  // Ref-based zoom callbacks so CanvasView can register them without causing
  // re-renders on every flyToState change.
  const zoomToNodeRef = useRef<(id: string) => void>(() => {});
  const zoomToLinkRef = useRef<(id: string) => void>(() => {});
  const zoomToNode = useCallback((id: string) => zoomToNodeRef.current(id), []);
  const zoomToLink = useCallback((id: string) => zoomToLinkRef.current(id), []);
  const setZoomCallbacks = useCallback(
    (nodeZoom: (id: string) => void, linkZoom: (id: string) => void) => {
      zoomToNodeRef.current = nodeZoom;
      zoomToLinkRef.current = linkZoom;
    },
    [],
  );

  // Stable refs so callbacks don't go stale when selection changes.
  const nodeIdRef = useRef<string | null>(null);
  const linkIdRef = useRef<string | null>(null);
  nodeIdRef.current = selectedNodeId;
  linkIdRef.current = selectedLinkId;

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
    setInspectorView("link");
  }, []);

  const clearSelection = useCallback(() => {
    setSelectedNodeId(null);
    setSelectedLinkId(null);
    setInspectorView("closed");
  }, []);

  // Memoized so provider-parent renders don't hand every consumer a fresh
  // context value (the sim arrays alone make consumer re-renders expensive).
  const value = useMemo(
    () => ({
      selectedNodeId,
      selectedLinkId,
      inspectorView,
      selectNode,
      selectLink,
      setInspectorView,
      setSelectedNodeId,
      setSelectedLinkId,
      clearSelection,
      simNodes,
      simLinks,
      setSimData,
      zoomToNode,
      zoomToLink,
      setZoomCallbacks,
    }),
    [
      selectedNodeId,
      selectedLinkId,
      inspectorView,
      selectNode,
      selectLink,
      clearSelection,
      simNodes,
      simLinks,
      setSimData,
      zoomToNode,
      zoomToLink,
      setZoomCallbacks,
    ],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useCanvasSelection() {
  return useContext(Ctx);
}
