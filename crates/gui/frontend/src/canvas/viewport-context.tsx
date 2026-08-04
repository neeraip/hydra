/**
 * Which elements the map is currently showing.
 *
 * The network list can dim rows whose element is outside the map view, so
 * "it exists, it just isn't where you're looking" is visible at a glance.
 * That needs the canvas's viewport and the canvas's *display* coordinates,
 * both of which live in `MapCanvas` — so the canvas registers a probe here
 * and the list asks it.
 *
 * The channel is a probe rather than a set of visible ids on purpose. The
 * list virtualizes, so only ~25 rows are on screen and only those need an
 * answer; computing a 46k-element set on every pan to answer 25 questions
 * is work nobody reads.
 *
 * Split into state and actions for the same reason as the hover channel:
 * the key changes on every (throttled) map move, and `MapCanvas` — which
 * goes to some length *not* to re-render while panning — must be able to
 * publish without subscribing.
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
import type { ElementClass } from "../hooks";

/** True when the element is inside the current map viewport. */
export type ViewportProbe = (cls: ElementClass, id: string) => boolean;

export interface ViewportBox {
  west: number;
  south: number;
  east: number;
  north: number;
}

interface ViewportActions {
  /** The canvas registers its probe, or `null` when it is not showing a
   * geographic map (schematic, local grid) and the question is meaningless. */
  setViewportProbe: (probe: ViewportProbe | null) => void;
  /** The canvas reports that the viewport moved. Throttle at the caller. */
  viewportMoved: () => void;
  /** Stable. Answers `true` when no probe is registered, so a caller that
   * forgets to check `viewportKey` dims nothing rather than everything. */
  isInViewport: ViewportProbe;
}

const KeyCtx = createContext<number | null>(null);
const ActionsCtx = createContext<ViewportActions>({
  setViewportProbe: () => {},
  viewportMoved: () => {},
  isInViewport: () => true,
});

export function ViewportProvider({ children }: { children: ReactNode }) {
  // `null` means no geographic viewport exists — the signal consumers use to
  // hide the feature rather than offer a toggle that does nothing.
  const [viewportKey, setViewportKey] = useState<number | null>(null);
  const probeRef = useRef<ViewportProbe | null>(null);

  const setViewportProbe = useCallback((probe: ViewportProbe | null) => {
    probeRef.current = probe;
    setViewportKey(probe ? (k) => (k ?? 0) + 1 : null);
  }, []);

  const viewportMoved = useCallback(() => {
    // Only meaningful while a probe is registered; a move reported after the
    // canvas left map mode must not resurrect the toggle.
    if (probeRef.current) setViewportKey((k) => (k ?? 0) + 1);
  }, []);

  const isInViewport = useCallback<ViewportProbe>(
    (cls, id) => probeRef.current?.(cls, id) ?? true,
    [],
  );

  const actions = useMemo(
    () => ({ setViewportProbe, viewportMoved, isInViewport }),
    [setViewportProbe, viewportMoved, isInViewport],
  );

  return (
    <ActionsCtx.Provider value={actions}>
      <KeyCtx.Provider value={viewportKey}>{children}</KeyCtx.Provider>
    </ActionsCtx.Provider>
  );
}

/**
 * Bumps whenever the map viewport moves; `null` when the canvas is not
 * showing a geographic map. Read this to re-render on pan — and to decide
 * whether to offer viewport-dependent UI at all.
 */
export function useViewportKey(): number | null {
  return useContext(KeyCtx);
}

/** Stable for the life of the provider. */
export function useViewportActions(): ViewportActions {
  return useContext(ActionsCtx);
}

// ── Geometry ──────────────────────────────────────────────────────────────────

export function pointInBox(x: number, y: number, b: ViewportBox): boolean {
  return x >= b.west && x <= b.east && y >= b.south && y <= b.north;
}

/**
 * Segment against an axis-aligned box, by Liang–Barsky clipping.
 *
 * Testing endpoints alone is the tempting shortcut and it is wrong: a trunk
 * main crossing the whole screen has both endpoints outside the view and is
 * the most visible thing on it.
 */
export function segmentIntersectsBox(
  x0: number,
  y0: number,
  x1: number,
  y1: number,
  b: ViewportBox,
): boolean {
  const dx = x1 - x0;
  const dy = y1 - y0;
  const p = [-dx, dx, -dy, dy];
  const q = [x0 - b.west, b.east - x0, y0 - b.south, b.north - y0];
  let t0 = 0;
  let t1 = 1;
  for (let i = 0; i < 4; i += 1) {
    if (p[i] === 0) {
      // Parallel to this edge: outside it means no intersection at all.
      if (q[i] < 0) return false;
      continue;
    }
    const r = q[i] / p[i];
    if (p[i] < 0) {
      if (r > t1) return false;
      if (r > t0) t0 = r;
    } else {
      if (r < t0) return false;
      if (r < t1) t1 = r;
    }
  }
  return true;
}

/** Any part of a polyline inside the box. */
export function pathIntersectsBox(
  path: ReadonlyArray<readonly [number, number]>,
  b: ViewportBox,
): boolean {
  if (path.length === 0) return false;
  if (path.length === 1) return pointInBox(path[0][0], path[0][1], b);
  for (let i = 1; i < path.length; i += 1) {
    const a = path[i - 1];
    const c = path[i];
    if (segmentIntersectsBox(a[0], a[1], c[0], c[1], b)) return true;
  }
  return false;
}

/**
 * A ring's bounding box against the viewport.
 *
 * Deliberately the bbox and not the polygon: it over-includes a catchment
 * whose bbox clips the view while its boundary does not, and over-including
 * costs a row its dimming — far cheaper than the alternative error of
 * hiding a catchment that is on screen.
 */
export function ringIntersectsBox(
  ring: ReadonlyArray<readonly [number, number]>,
  b: ViewportBox,
): boolean {
  if (ring.length === 0) return false;
  let minX = Number.POSITIVE_INFINITY;
  let minY = Number.POSITIVE_INFINITY;
  let maxX = Number.NEGATIVE_INFINITY;
  let maxY = Number.NEGATIVE_INFINITY;
  for (const [x, y] of ring) {
    if (x < minX) minX = x;
    if (x > maxX) maxX = x;
    if (y < minY) minY = y;
    if (y > maxY) maxY = y;
  }
  return minX <= b.east && maxX >= b.west && minY <= b.north && maxY >= b.south;
}
