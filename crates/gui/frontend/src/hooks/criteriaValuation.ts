/**
 * The engine-generic criteria valuation (hydra-common §7.3) a project has
 * saved: opaque JSON, keyed by the criterion keys its engine catalogs.
 *
 * One store per project, shared by everyone reading it — the same reason
 * the wds criteria store exists. Every project view is mounted at once
 * (hidden with `display: none` rather than unmounted), so component-local
 * state gave the toolbar's editor and the Analysis page a copy each, and
 * an edit in one would never reach the other.
 *
 * The wds standard keeps its own typed store (`useProjectCriteria`): the
 * canvas colours by those fields directly, and they predate the contract.
 * Every engine without that history lands here.
 */

import { useCallback, useEffect, useSyncExternalStore } from "react";
import type { Valuation } from "../components/analysis/criteria";
import { invoke, tryInvokeResult } from "./ipc";

/** The saved valuation, or `null` for none saved; `undefined` when the
 * question could not be answered — an unreadable file must not be taken
 * for "none saved", which would write defaults over what is on disk. */
async function getCriteriaValuation(
  projectId: string,
): Promise<Valuation | null | undefined> {
  const read = await tryInvokeResult<Valuation | null>(
    "get_criteria_valuation",
    { projectId },
  );
  return read.ok ? read.value : undefined;
}

async function updateCriteriaValuation(
  projectId: string,
  valuation: Valuation,
): Promise<void> {
  await invoke("update_criteria_valuation", { projectId, valuation });
}

const cache = new Map<string, Valuation>();
const inFlight = new Set<string>();
const listeners = new Set<() => void>();

function notify(): void {
  for (const l of listeners) l();
}

function subscribe(cb: () => void): () => void {
  listeners.add(cb);
  return () => {
    listeners.delete(cb);
  };
}

/** Fetch once per project, however many readers ask. */
function ensureLoaded(projectId: string): void {
  if (cache.has(projectId) || inFlight.has(projectId)) return;
  inFlight.add(projectId);
  void getCriteriaValuation(projectId).then((read) => {
    inFlight.delete(projectId);
    // Unreadable: not cached, so the next reader retries rather than
    // inheriting the failure.
    if (read === undefined) return;
    // None saved is an empty valuation, which is exactly "every criterion
    // at its catalog default" (hydra-common §7.3).
    cache.set(projectId, read ?? {});
    notify();
  });
}

/**
 * The active project's criteria valuation, with a setter that persists.
 *
 * `undefined` until the saved valuation is known — callers forwarding it
 * to block production must wait rather than send `{}`, because an empty
 * valuation is a *decision* (every criterion at its default) and would
 * override the criteria sitting on disk.
 */
export function useCriteriaValuation(projectId: string | null): {
  valuation: Valuation | undefined;
  setValuation: (next: Valuation) => void;
} {
  useEffect(() => {
    if (projectId) ensureLoaded(projectId);
  }, [projectId]);

  const snapshot = useCallback(
    () => (projectId ? cache.get(projectId) : undefined),
    [projectId],
  );
  const valuation = useSyncExternalStore(subscribe, snapshot, snapshot);

  const setValuation = useCallback(
    (next: Valuation) => {
      if (!projectId) return;
      // Applied locally first so every reader repaints on the edit's own
      // frame; the write is fire-and-forget because a failure loses a
      // preference, not model data.
      cache.set(projectId, next);
      notify();
      void updateCriteriaValuation(projectId, next);
    },
    [projectId],
  );

  return { valuation, setValuation };
}
