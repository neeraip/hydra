/**
 * Minimal context carrying the timeline's current reporting-period index
 * (CanvasView-local scrub state) down to deep consumers — today only the
 * inspector's TimeSeriesCard, which draws a scrub marker on its sparklines.
 *
 * The value is a primitive (`number | null`), so scrub re-renders stay
 * contained to the components that actually call `useCurrentPeriod()`.
 * Default is `null` (no provider / no timeline): consumers render no marker.
 */

import { createContext, type ReactNode, useContext, useState } from "react";

const CurrentPeriodCtx = createContext<number | null>(null);

export function CurrentPeriodProvider({
  period,
  children,
}: {
  period: number | null;
  children: ReactNode;
}) {
  return (
    <CurrentPeriodCtx.Provider value={period}>
      {children}
    </CurrentPeriodCtx.Provider>
  );
}

/** Current reporting-period index, or `null` outside a timeline context. */
export function useCurrentPeriod(): number | null {
  return useContext(CurrentPeriodCtx);
}

// ── Project-level period state ────────────────────────────────────────────────
//
// The timeline lives in the canvas, but "which period am I looking at?" is a
// question the whole project view shares — the element tables answer it too.
// Rather than lift the scrub state itself (and its playback, clamping and
// preference logic) out of the canvas, the canvas keeps owning it and
// publishes the value here, so sibling views read one number without the
// canvas having to be mounted for them to have it.

const SetPeriodCtx = createContext<(p: number | null) => void>(() => {});

export function ProjectPeriodProvider({ children }: { children: ReactNode }) {
  const [period, setPeriod] = useState<number | null>(null);
  return (
    <SetPeriodCtx.Provider value={setPeriod}>
      <CurrentPeriodCtx.Provider value={period}>
        {children}
      </CurrentPeriodCtx.Provider>
    </SetPeriodCtx.Provider>
  );
}

/** Publish the current period; called by whichever view owns the timeline. */
export function usePublishCurrentPeriod(): (p: number | null) => void {
  return useContext(SetPeriodCtx);
}
