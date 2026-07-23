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
  model: boolean; // Base model nodes + links
  nodeLabels: boolean; // Node label text
  linkLabels: boolean; // Link label text
  pressZone: boolean; // Pressure zone overlay (future)
  measure: boolean; // Measurement data overlay (future)
}

interface CanvasLayersCtx {
  layers: CanvasLayers;
  setLayer: (id: keyof CanvasLayers, on: boolean) => void;
}

const DEFAULT: CanvasLayers = {
  model: true,
  nodeLabels: false,
  linkLabels: false,
  pressZone: false,
  measure: false,
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
