/**
 * Canvas layer visibility context.
 *
 * Allows `CanvasView` (which hosts the canvas) and `CanvasRail` (inside
 * `SecondaryRail`, a sibling in the App tree) to share the same layer
 * visibility state without prop-drilling through `App.tsx`.
 */

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useMemo,
  useState,
} from "react";

export interface CanvasLayers {
  // Nodes and links toggle separately. One "base model" switch could only turn
  // the network off wholesale, and the common want is to drop one so the other
  // is legible — links in a dense area, or nodes when tracing connectivity.
  //
  // Deliberately node/link rather than element type: that is the one split that
  // carries over to a drainage network, which also has nodes (junctions,
  // outfalls, storage) and links (conduits, weirs, orifices). Filtering by type
  // or attribute needs the element schema `hydra-common` defers until a second
  // engine exists.
  nodes: boolean;
  links: boolean;
  /** Areal region polygons (subcatchments). Only meaningful for models
   * that carry any; map mode only (rings are source-CRS geometry the
   * schematic layout knows nothing about). */
  regions: boolean;
  nodeLabels: boolean; // Node label text
  linkLabels: boolean; // Link label text
}

interface CanvasLayersCtx {
  layers: CanvasLayers;
  setLayer: (id: keyof CanvasLayers, on: boolean) => void;
}

// Only layers the toolbar actually offers. `pressZone` and `measure` sat
// here as declared-and-defaulted placeholders for overlays that were never
// built, so the type promised two toggles the app had no way to reach —
// and `pressZone` named a water-distribution concept in the one file that
// documents itself as engine-neutral.
const DEFAULT: CanvasLayers = {
  nodes: true,
  links: true,
  regions: true,
  nodeLabels: false,
  linkLabels: false,
};

const Ctx = createContext<CanvasLayersCtx>({
  layers: DEFAULT,
  setLayer: () => {},
});

export function CanvasLayersProvider({ children }: { children: ReactNode }) {
  const [layers, setLayers] = useState<CanvasLayers>(DEFAULT);
  const setLayer = useCallback((id: keyof CanvasLayers, on: boolean) => {
    setLayers((prev) => ({ ...prev, [id]: on }));
  }, []);
  const value = useMemo<CanvasLayersCtx>(
    () => ({ layers, setLayer }),
    [layers, setLayer],
  );
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useCanvasLayers() {
  return useContext(Ctx);
}
