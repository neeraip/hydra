/**
 * Minimal context that tracks how many times the network in Tauri managed
 * state has structurally changed. `NetworkDataProvider` re-fetches the full
 * snapshot, and version-keyed hooks (`usePatterns`, `useKindElements`,
 * `useKindCounts`, `useCollectionDetail`) re-fetch their rows, whenever the
 * version bumps.
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
  /**
   * Clear the stale flag after a successful run for that scenario.
   *
   * `runStartedAt` is when the run that is clearing it began, in epoch
   * seconds. An edit made *while* a run was in flight is not answered by
   * that run — the solver read the model before it happened — so the
   * flag survives. Without this, a value-only edit during a long run
   * left results that looked current and described the model as it was
   * before, and the topology digest cannot see it because changing a
   * diameter changes no topology.
   */
  clearEdited: (
    projectId: string,
    scenarioId: string | null,
    runStartedAt?: number | null,
  ) => void;
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
  // When each target was edited, in epoch seconds — not merely *that* it
  // was. A run only answers the edits that preceded it, and the answer to
  // "may I clear this" is a comparison of two times.
  const [editedTargets, setEditedTargets] = useState<
    ReadonlyMap<string, number>
  >(new Map());

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
      // The *first* edit since the last run is the one whose time
      // matters: a later one is no less answered by a run that started
      // before either. Keeping the earliest also makes the comparison
      // stable while someone types.
      setEditedTargets((prev) => {
        if (prev.has(key)) return prev;
        const next = new Map(prev);
        next.set(key, Math.floor(Date.now() / 1000));
        return next;
      });
    },
    [],
  );

  const clearEdited = useCallback(
    (
      projectId: string,
      scenarioId: string | null,
      runStartedAt?: number | null,
    ) => {
      const key = editedKey(projectId, scenarioId);
      setEditedTargets((prev) => {
        const editedAt = prev.get(key);
        if (editedAt === undefined) return prev;
        if (!runSupersedesEdit(editedAt, runStartedAt)) return prev;
        const next = new Map(prev);
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

/**
 * Whether a finished run answers an edit, so its stale marker can go.
 *
 * A run reads the model when it starts. An edit made while it was in
 * flight is not in the results it produced, so the run does not answer
 * it and the marker has to survive — otherwise a value-only edit during
 * a long run leaves results that look current and describe the model as
 * it was. The topology digest cannot cover this: changing a diameter
 * changes no topology, so that check sees nothing and the marker was the
 * only signal.
 *
 * An edit in the same second as the start counts as *after* it. The two
 * clocks are both epoch seconds and a second is coarse enough to
 * straddle, so the tie goes to warning rather than to silence.
 *
 * A run with no start time — cancelled before it began, or an item from
 * an older queue — answers nothing.
 */
export function runSupersedesEdit(
  editedAt: number,
  runStartedAt: number | null | undefined,
): boolean {
  if (runStartedAt == null) return false;
  return editedAt < runStartedAt;
}
