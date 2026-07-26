import { useCallback } from "react";
import { useAppState } from "../AppContext";
import { useNetworkVersion } from "./NetworkVersionContext";
import { renameElement } from "./network";
import { saveProjectOnDisk } from "./projects";
import { clearStacks, stackKey } from "./undoStack";

/**
 * Shared element-rename flow used by every rename entry point (canvas
 * inspector, editor rename action). Fires the immediate `rename_element`
 * command, then reconciles the side effects that rename entails:
 *
 *  - clears the per-(project, scenario) undo stack — its entries key on the
 *    old element id and would fail to apply after the rename (see undoStack),
 *  - persists the mutated network to disk,
 *  - marks the active scenario's results stale,
 *  - toasts success or the backend error.
 *
 * Returns `true` only when the rename actually committed, so callers can do
 * their own follow-up (e.g. re-select the element under its new id). A no-op
 * rename (empty or unchanged id) returns `false` without touching anything.
 */
export function useElementRename(): (
  kind: string,
  oldId: string,
  rawNewId: string,
) => Promise<boolean> {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();
  const { markEdited } = useNetworkVersion();

  return useCallback(
    async (kind, oldId, rawNewId) => {
      const newId = rawNewId.trim();
      if (!newId || newId === oldId) return false;
      try {
        await renameElement(kind, oldId, newId);
        if (activeProjectId) {
          clearStacks(stackKey(activeProjectId, activeScenarioId ?? null));
          await saveProjectOnDisk(activeProjectId, activeScenarioId);
        }
        markEdited(activeScenarioId);
        showToast(
          `Renamed ${oldId} → ${newId}. Undo history cleared; results marked stale.`,
          "success",
        );
        return true;
      } catch (err) {
        showToast(
          typeof err === "string" ? err : `Could not rename ${oldId}`,
          "error",
        );
        return false;
      }
    },
    [activeProjectId, activeScenarioId, markEdited, showToast],
  );
}
