/**
 * Minimal context that tracks how many times a new network has been loaded
 * into Tauri managed state. `useNodes` and `useLinks` subscribe to this so
 * they re-fetch from `get_nodes` / `get_links` whenever the version bumps.
 *
 * Also tracks which scenario IDs have had their network edited since the last
 * successful simulation run, so the canvas can show a "stale results" warning.
 * `null` in the set means the base model (no scenario selected).
 *
 * Kept in a standalone file to avoid a circular dependency between
 * `data/index.ts` (which calls `useNetworkVersion`) and `state.tsx` (which
 * imports from `data/index.ts`).
 */

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { listenNetworkChanged } from "./index";

interface NetworkVersionCtx {
  version: number;
  bumpNetwork: () => void;
  /** Scenario IDs (or null for base model) whose network was edited after the last run. */
  editedScenarioIds: ReadonlySet<string | null>;
  /** Mark a scenario's results as stale because its network was edited. */
  markEdited: (scenarioId: string | null) => void;
  /** Clear the stale flag after a successful simulation run for that scenario. */
  clearEdited: (scenarioId: string | null) => void;
}

const Ctx = createContext<NetworkVersionCtx>({
  version: 0,
  bumpNetwork: () => {},
  editedScenarioIds: new Set(),
  markEdited: () => {},
  clearEdited: () => {},
});

/**
 * Wrap `fn` so that any number of synchronous calls to the returned function
 * coalesce into a single `fn()` invocation on the next microtask. Calls made
 * in later tasks (after the microtask has flushed) schedule a fresh
 * invocation. This is the scheduling primitive behind `bumpNetwork`: bumps
 * arriving in the same tick (e.g. the backend `network-changed` event landing
 * alongside a manual bump from a canvas handler) produce one version
 * increment, so subscribers refetch the network snapshot once per batch.
 */
export function makeCoalescedScheduler(fn: () => void): () => void {
  let pending = false;
  return () => {
    if (pending) return;
    pending = true;
    queueMicrotask(() => {
      pending = false;
      fn();
    });
  };
}

export function NetworkVersionProvider({ children }: { children: ReactNode }) {
  const [version, setVersion] = useState(0);
  const [editedScenarioIds, setEditedScenarioIds] = useState<
    ReadonlySet<string | null>
  >(new Set());

  // Coalesce bumps arriving in the same tick into a single version increment
  // (see makeCoalescedScheduler). useMemo keeps the callback identity stable
  // across renders, like the previous useCallback + ref implementation.
  const bumpNetwork = useMemo(
    () => makeCoalescedScheduler(() => setVersion((v) => v + 1)),
    [],
  );

  // Keep all windows in sync: whenever the backend emits network-changed
  // (from patch_element, patch_node_position, or delete_element), bump the
  // local version so useNodes/useLinks re-fetch automatically.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listenNetworkChanged(() => bumpNetwork()).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [bumpNetwork]);

  const markEdited = useCallback((scenarioId: string | null) => {
    setEditedScenarioIds((prev) => {
      if (prev.has(scenarioId)) return prev;
      const next = new Set(prev);
      next.add(scenarioId);
      return next;
    });
  }, []);

  const clearEdited = useCallback((scenarioId: string | null) => {
    setEditedScenarioIds((prev) => {
      if (!prev.has(scenarioId)) return prev;
      const next = new Set(prev);
      next.delete(scenarioId);
      return next;
    });
  }, []);

  const value = useMemo<NetworkVersionCtx>(
    () => ({
      version,
      bumpNetwork,
      editedScenarioIds,
      markEdited,
      clearEdited,
    }),
    [version, bumpNetwork, editedScenarioIds, markEdited, clearEdited],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useNetworkVersion() {
  return useContext(Ctx);
}
