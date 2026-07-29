/**
 * Minimal context that tracks how many times the network in Tauri managed
 * state has structurally changed. `NetworkDataProvider` re-fetches the full
 * snapshot, and version-keyed hooks (`usePatterns` / `useCurves` /
 * `useControls` / `useRules`) re-fetch their rows, whenever the version
 * bumps.
 *
 * Also tracks which scenario IDs have had their network edited since the last
 * successful simulation run, so the canvas can show a "stale results" warning.
 * `null` in the set means the base model (no scenario selected).
 *
 * Kept in a standalone file to avoid a circular dependency between the
 * `./index` barrel (whose modules call `useNetworkVersion`) and
 * `AppContext.tsx` (which imports from the barrel) — so imports here must
 * come from concrete modules, never the `./index` barrel.
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
import {
  isStructuralNetworkChange,
  listenNetworkChangedPayload,
} from "./network";

interface NetworkVersionCtx {
  version: number;
  bumpNetwork: () => void;
  /** Scenario IDs (or null for base model) whose network was edited after the last run. */
  /** Mark a scenario's results as stale because its network was edited. */
  markEdited: (projectId: string, scenarioId: string | null) => void;
  /** Clear the stale flag after a successful simulation run for that scenario. */
  clearEdited: (projectId: string, scenarioId: string | null) => void;
  /** Whether that target has edits since its last run. */
  isEdited: (projectId: string | null, scenarioId: string | null) => boolean;
}

const Ctx = createContext<NetworkVersionCtx>({
  version: 0,
  bumpNetwork: () => {},
  markEdited: () => {},
  clearEdited: () => {},
  isEdited: () => false,
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

/**
 * Key for the edited-targets set.
 *
 * Project-qualified because the base model's scenario id is `null` in every
 * project: an unqualified set marked one project's base model as edited and
 * every other project's base model along with it, showing a false "stale"
 * badge and preflight warning after a switch.
 *
 * Keyed rather than reset on project change deliberately — the toolbar
 * re-seeds this set from persisted state when a project loads, so a reset
 * would have to be ordered against that seeding, and losing the race would
 * silently discard the new project's real stale flags.
 */
export function editedKey(
  projectId: string,
  scenarioId: string | null,
): string {
  // Scenarios carry an `s:` marker so no scenario id can ever equal the base
  // sentinel. Ids are UUIDs today, so a scenario literally called "base" is
  // impossible — but "that value can't collide" is the assumption `null` broke
  // in the first place, so the encoding does not rely on it.
  return scenarioId === null
    ? `${projectId}:base`
    : `${projectId}:s:${scenarioId}`;
}

export function NetworkVersionProvider({ children }: { children: ReactNode }) {
  const [version, setVersion] = useState(0);
  const [editedTargets, setEditedTargets] = useState<ReadonlySet<string>>(
    new Set(),
  );

  // Coalesce bumps arriving in the same tick into a single version increment
  // (see makeCoalescedScheduler). useMemo keeps the callback identity stable
  // across renders, like the previous useCallback + ref implementation.
  const bumpNetwork = useMemo(
    () => makeCoalescedScheduler(() => setVersion((v) => v + 1)),
    [],
  );

  // Keep all windows in sync: whenever the backend emits a *structural*
  // network-changed event (create/delete/pattern/curve/control — no element
  // payload), bump the local version so version-keyed hooks re-fetch.
  // Element-scoped deltas carry the updated DTOs and are self-applied by
  // NetworkDataContext's own listener; bumping on them made every
  // version-keyed hook (patterns/curves/controls/rules) refetch data the
  // delta already contained.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let disposed = false;
    listenNetworkChangedPayload((payload) => {
      if (isStructuralNetworkChange(payload)) bumpNetwork();
    }).then((fn) => {
      // StrictMode double-mount: the first effect's cleanup can run before
      // this promise resolves — dispose the late listener instead of
      // leaking it (which doubled every bump).
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [bumpNetwork]);

  const markEdited = useCallback(
    (projectId: string, scenarioId: string | null) => {
      const key = editedKey(projectId, scenarioId);
      setEditedTargets((prev) => {
        if (prev.has(key)) return prev;
        const next = new Set(prev);
        next.add(key);
        return next;
      });
    },
    [],
  );

  const clearEdited = useCallback(
    (projectId: string, scenarioId: string | null) => {
      const key = editedKey(projectId, scenarioId);
      setEditedTargets((prev) => {
        if (!prev.has(key)) return prev;
        const next = new Set(prev);
        next.delete(key);
        return next;
      });
    },
    [],
  );

  const isEdited = useCallback(
    (projectId: string | null, scenarioId: string | null) =>
      projectId != null && editedTargets.has(editedKey(projectId, scenarioId)),
    [editedTargets],
  );

  const value = useMemo<NetworkVersionCtx>(
    () => ({
      version,
      bumpNetwork,
      markEdited,
      clearEdited,
      isEdited,
    }),
    [version, bumpNetwork, markEdited, clearEdited, isEdited],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useNetworkVersion() {
  return useContext(Ctx);
}
