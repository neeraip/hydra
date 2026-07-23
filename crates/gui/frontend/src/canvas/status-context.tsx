/**
 * Canvas status context.
 *
 * Carries lightweight canvas-level status flags (currently: coordinate
 * coverage) from CanvasView (writer, deep in the tree) up to ProjectPage's
 * TopBar (reader, higher in the tree) without prop-drilling.
 *
 * Follows the same pattern as `layers-context.tsx` — registered once at the
 * root of the app in `main.tsx`.
 */

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useMemo,
  useState,
} from "react";

export type CoordStatus = "complete" | "partial" | "empty";

export interface CanvasStatus {
  /** Coordinate coverage classification for the currently loaded network. */
  coordStatus: CoordStatus;
  /** Number of nodes with missing coordinates (x === 0 && y === 0). */
  coordMissingCount: number;
  /** Total node count of the currently loaded network. */
  coordTotalCount: number;
}

interface CanvasStatusCtx extends CanvasStatus {
  setCoordStatus: (
    status: CoordStatus,
    missingCount: number,
    totalCount: number,
  ) => void;
}

const DEFAULT: CanvasStatus = {
  coordStatus: "complete",
  coordMissingCount: 0,
  coordTotalCount: 0,
};

const Ctx = createContext<CanvasStatusCtx>({
  ...DEFAULT,
  setCoordStatus: () => {},
});

export function CanvasStatusProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<CanvasStatus>(DEFAULT);

  const setCoordStatus = useCallback(
    (
      coordStatus: CoordStatus,
      coordMissingCount: number,
      coordTotalCount: number,
    ) => {
      setStatus({ coordStatus, coordMissingCount, coordTotalCount });
    },
    [],
  );

  const value = useMemo<CanvasStatusCtx>(
    () => ({ ...status, setCoordStatus }),
    [status, setCoordStatus],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useCanvasStatus() {
  return useContext(Ctx);
}
