import { useCallback } from "react";
import { useAppState } from "../AppContext";
import { useNetworkVersion } from "./NetworkVersionContext";
import { setElementAttribute } from "./network";
import { saveProjectOnDisk } from "./projects";

/**
 * Shared attribute-write flow for every place a model number is edited —
 * the element inspector's Properties rows and the Editor's per-kind
 * tables.
 *
 * A write is three things, and only the first is the command:
 *
 *  - set the value on the in-memory model,
 *  - persist it, because an edit that lives only in memory is lost when
 *    the app closes and the user has no way to know that,
 *  - mark the scenario's results stale, because a changed invert or
 *    roughness makes the run that is on screen describe a model that no
 *    longer exists.
 *
 * The same three that `useElementRename` and the canvas's move do. They
 * are gathered here so a second editing surface cannot ship with only
 * the first of them, which is exactly how the inspector shipped.
 *
 * Failures are toasted and rethrown: the caller's field restores the
 * value the model still holds, so the toast says what happened and the
 * field says what is true.
 */
export function useElementAttributeWrite(): (
  elementId: string,
  key: string,
  value: number,
) => Promise<void> {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();
  const { markEdited } = useNetworkVersion();

  return useCallback(
    async (elementId, key, value) => {
      try {
        await setElementAttribute(elementId, key, value);
      } catch (err) {
        showToast(
          typeof err === "string"
            ? err
            : `Could not set ${key} on ${elementId}`,
          "error",
        );
        throw err;
      }
      if (activeProjectId) {
        await saveProjectOnDisk(activeProjectId, activeScenarioId);
        markEdited(activeProjectId, activeScenarioId);
      }
    },
    [activeProjectId, activeScenarioId, markEdited, showToast],
  );
}
