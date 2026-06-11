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

export function NetworkVersionProvider({ children }: { children: ReactNode }) {
  const [version, setVersion] = useState(0);
  const [editedScenarioIds, setEditedScenarioIds] = useState<
    ReadonlySet<string | null>
  >(new Set());

  const bumpNetwork = useCallback(() => setVersion((v) => v + 1), []);

  // Keep all windows in sync: whenever the backend emits network-changed
  // (from patch_element, patch_node_position, or delete_element), bump the
  // local version so useNodes/useLinks re-fetch automatically.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listenNetworkChanged(() => setVersion((v) => v + 1)).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

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

  return (
    <Ctx.Provider
      value={{
        version,
        bumpNetwork,
        editedScenarioIds,
        markEdited,
        clearEdited,
      }}
    >
      {children}
    </Ctx.Provider>
  );
}

export function useNetworkVersion() {
  return useContext(Ctx);
}
