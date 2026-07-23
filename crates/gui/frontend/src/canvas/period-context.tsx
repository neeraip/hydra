/**
 * Minimal context carrying the timeline's current reporting-period index
 * (CanvasView-local scrub state) down to deep consumers — today only the
 * inspector's TimeSeriesCard, which draws a scrub marker on its sparklines.
 *
 * The value is a primitive (`number | null`), so scrub re-renders stay
 * contained to the components that actually call `useCurrentPeriod()`.
 * Default is `null` (no provider / no timeline): consumers render no marker.
 */

import { createContext, type ReactNode, useContext } from "react";

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
