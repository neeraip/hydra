/**
 * Applies undo/redo entries from `undoStack.ts` against the backend and
 * exposes the ⌘Z / ⌘⇧Z callbacks App.tsx binds.
 *
 * Application contract:
 *   - An `EditSet` is applied strictly recreate → patch → delete, each step
 *     through the existing mutation wrappers; the resulting `network-changed`
 *     events refresh frontend state (structural mutations emit the
 *     full-refetch signal, `patch_elements` emits deltas).
 *   - Any step failure aborts the apply, toasts the error, and DROPS the
 *     entry (no retry): the entry was already popped, and it is never pushed
 *     to the opposite stack, so both stacks stay consistent.
 *   - After an apply (even a partial failure — some steps may have mutated
 *     in-memory state) the project is re-saved to disk and the scenario is
 *     marked edited, mirroring the canvas commit paths.
 */

import { useCallback, useRef } from "react";
import { useAppState } from "../AppContext";
import { invoke } from "./ipc";
import { useNetworkVersion } from "./NetworkVersionContext";
import { createLink, createNode, patchElements } from "./network";
import { saveProjectOnDisk } from "./projects";
import {
  type EditSet,
  type FieldPatch,
  pushRedoEntry,
  type RecreateSpec,
  restoreUndoEntry,
  stackKey,
  takeRedo,
  takeUndo,
} from "./undoStack";

/** `patch_elements` reports per-item errors without rejecting — surface the
 *  first one as a throw so the apply loop aborts like any other failure. */
async function applyFieldPatches(patches: FieldPatch[]): Promise<void> {
  if (patches.length === 0) return;
  const result = await patchElements(patches);
  if (result.errors.length > 0) {
    throw new Error(result.errors[0]);
  }
}

async function applyRecreate(spec: RecreateSpec): Promise<void> {
  if (spec.elementType === "node") {
    await createNode(
      spec.kind,
      spec.id,
      spec.x,
      spec.y,
      spec.elevation,
      spec.minLevel,
      spec.maxLevel,
      spec.initialLevel,
    );
  } else {
    await createLink(spec.kind, spec.id, spec.fromId, spec.toId);
  }
  await applyFieldPatches(spec.patches);
}

/** Apply one edit set in recreate → patch → delete order. Throws on the
 *  first failed step. Exported for tests. */
export async function applyEditSet(set: EditSet): Promise<void> {
  for (const spec of set.recreates ?? []) {
    await applyRecreate(spec);
  }
  await applyFieldPatches(set.patches ?? []);
  for (const d of set.deletes ?? []) {
    // The throwing invoke, not the silent `deleteElement` wrapper — a failed
    // delete must abort the apply and surface its error.
    await invoke<void>("delete_element", { kind: d.kind, id: d.id });
  }
}

export function useUndoRedo(): { undo: () => void; redo: () => void } {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();
  const { markEdited } = useNetworkVersion();
  // Serialise applies: rapid ⌘Z must not overlap an in-flight apply.
  const busyRef = useRef(false);

  const run = useCallback(
    async (direction: "undo" | "redo") => {
      if (!activeProjectId || busyRef.current) return;
      const key = stackKey(activeProjectId, activeScenarioId ?? null);
      const entry = direction === "undo" ? takeUndo(key) : takeRedo(key);
      if (!entry) return; // empty stack — deliberately no toast
      busyRef.current = true;
      let mutated = false;
      try {
        mutated = true;
        await applyEditSet(direction === "undo" ? entry.undo : entry.redo);
        if (direction === "undo") pushRedoEntry(key, entry);
        else restoreUndoEntry(key, entry);
        showToast(
          `${direction === "undo" ? "Undid" : "Redid"}: ${entry.label}`,
          "success",
        );
      } catch (err) {
        // Entry already popped and never re-pushed — dropped by design.
        const msg = err instanceof Error ? err.message : String(err);
        showToast(
          `${direction === "undo" ? "Undo" : "Redo"} failed: ${msg}`,
          "error",
        );
      } finally {
        if (mutated) {
          // Persist whatever was applied (partial failures included) so the
          // on-disk INP matches in-memory state, like every commit path.
          await saveProjectOnDisk(activeProjectId, activeScenarioId ?? null);
          markEdited(activeScenarioId ?? null);
        }
        busyRef.current = false;
      }
    },
    [activeProjectId, activeScenarioId, markEdited, showToast],
  );

  const undo = useCallback(() => void run("undo"), [run]);
  const redo = useCallback(() => void run("redo"), [run]);
  return { undo, redo };
}
