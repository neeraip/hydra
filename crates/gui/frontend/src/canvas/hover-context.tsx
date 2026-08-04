/**
 * Which element the pointer is over, wherever the pointer happens to be.
 *
 * Hovering a row in the network list, or a connected element in the
 * inspector, should light that element up on the canvas exactly as hovering
 * the element itself does. That needs one shared answer to "what is hovered",
 * which is what this holds.
 *
 * It is deliberately **not** part of the selection context. That value object
 * carries the sim-merged element arrays, so writing to it on every mousemove
 * would hand ~46k-element arrays to every consumer several times a second.
 *
 * For the same reason the channel is split in two:
 *
 *   `useHoverActions()`  stable for the life of the provider — a component
 *                        that only *sets* hover (a list row, an inspector
 *                        link) never re-renders because of hover at all.
 *   `useHoverState()`    the current ids; re-renders on every change, so
 *                        only the canvas should read it.
 */

import {
  createContext,
  type ReactNode,
  useContext,
  useMemo,
  useState,
} from "react";

export interface HoverState {
  hoveredNodeId: string | null;
  hoveredLinkId: string | null;
  hoveredRegionId: string | null;
}

export interface HoverActions {
  /** Hover a point element, clearing any other hovered element. */
  hoverNode: (id: string | null) => void;
  hoverLink: (id: string | null) => void;
  hoverRegion: (id: string | null) => void;
  clearHover: () => void;
}

const EMPTY: HoverState = {
  hoveredNodeId: null,
  hoveredLinkId: null,
  hoveredRegionId: null,
};

const StateCtx = createContext<HoverState>(EMPTY);
const ActionsCtx = createContext<HoverActions>({
  hoverNode: () => {},
  hoverLink: () => {},
  hoverRegion: () => {},
  clearHover: () => {},
});

function isEmpty(s: HoverState): boolean {
  return (
    s.hoveredNodeId == null &&
    s.hoveredLinkId == null &&
    s.hoveredRegionId == null
  );
}

/**
 * The state transition for one hover setter, as a pure function.
 *
 * Setting a hover is **exclusive** — the pointer cannot be over a junction
 * and a conduit at once, and a stale glow reads as two hovered elements.
 *
 * Clearing is **not** exclusive, and that asymmetry is the whole subtlety
 * here: deck fires `onHover` per layer with no ordering guarantee, so moving
 * the pointer straight from a link onto a node can deliver the node's hover
 * *before* the link's null. A clear that wiped every class would cancel the
 * hover that had just arrived, and the glow would flicker on every crossing.
 * So clearing a class that is not currently hovered is a no-op.
 *
 * Returns `prev` unchanged whenever nothing would move, so a mousemove that
 * stays on one element never re-renders the canvas.
 */
export function nextHoverState(
  prev: HoverState,
  key: keyof HoverState,
  id: string | null,
): HoverState {
  if (id == null) {
    return prev[key] == null ? prev : { ...prev, [key]: null };
  }
  const alreadyOnlyThis =
    prev[key] === id &&
    (Object.keys(prev) as (keyof HoverState)[]).every(
      (k) => k === key || prev[k] == null,
    );
  return alreadyOnlyThis ? prev : { ...EMPTY, [key]: id };
}

export function HoverProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<HoverState>(EMPTY);

  const actions = useMemo<HoverActions>(() => {
    const set =
      (key: keyof HoverState) =>
      (id: string | null): void =>
        setState((prev) => nextHoverState(prev, key, id));
    return {
      hoverNode: set("hoveredNodeId"),
      hoverLink: set("hoveredLinkId"),
      hoverRegion: set("hoveredRegionId"),
      clearHover: () => setState((prev) => (isEmpty(prev) ? prev : EMPTY)),
    };
  }, []);

  return (
    <ActionsCtx.Provider value={actions}>
      <StateCtx.Provider value={state}>{children}</StateCtx.Provider>
    </ActionsCtx.Provider>
  );
}

/** The hovered ids. Re-renders on every hover change — canvas only. */
export function useHoverState(): HoverState {
  return useContext(StateCtx);
}

/** Setters only, stable for the life of the provider. */
export function useHoverActions(): HoverActions {
  return useContext(ActionsCtx);
}
