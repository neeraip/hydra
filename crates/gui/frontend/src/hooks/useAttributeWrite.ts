import { useCallback } from "react";
import { useAppState } from "../AppContext";
import { useNetworkVersion } from "./NetworkVersionContext";
import {
  type RecordSet,
  setCollectionContents,
  setElementAttribute,
  setElementEnds,
  setElementRecords,
} from "./network";
import { saveProjectOnDisk } from "./projects";
import { pushUndoEntry, stackKey } from "./undoStack";

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
 *    longer exists,
 *  - and capture the inverse, when the caller can say what the value
 *    was. A set is its own undo with the old number in it.
 *
 * The same three that `useElementRename` and the canvas's move do. They
 * are gathered here so a second editing surface cannot ship with only
 * the first of them, which is exactly how the inspector shipped.
 *
 * Failures are toasted and rethrown: the caller's field restores the
 * value the model still holds, so the toast says what happened and the
 * field says what is true.
 *
 * With no active project there is nothing to write to, so the write is
 * skipped rather than sent — the command needs the project to know
 * which engine's model it is addressing.
 */
export function useElementAttributeWrite(): (
  elementId: string,
  key: string,
  value: number | string,
  /** What the field showed before, so the write can be undone. Omit
   * and the edit is not captured — an inverse nobody can supply is
   * better absent than guessed. */
  previous?: number | string,
) => Promise<void> {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();
  const { markEdited } = useNetworkVersion();

  return useCallback(
    async (elementId, key, value, previous) => {
      if (!activeProjectId) return;
      try {
        await setElementAttribute(activeProjectId, elementId, key, value);
      } catch (err) {
        showToast(
          typeof err === "string"
            ? err
            : `Could not set ${key} on ${elementId}`,
          "error",
        );
        throw err;
      }
      if (previous != null) {
        pushUndoEntry(stackKey(activeProjectId, activeScenarioId ?? null), {
          label: `Changed ${key} on ${elementId}`,
          undo: {
            ops: [{ op: "set", id: elementId, key, value: previous }],
          },
          redo: { ops: [{ op: "set", id: elementId, key, value }] },
        });
      }
      await saveProjectOnDisk(activeProjectId, activeScenarioId);
      markEdited(activeProjectId, activeScenarioId);
    },
    [activeProjectId, activeScenarioId, markEdited, showToast],
  );
}

/**
 * The same flow for a line's two ends (hydra-common §4.5.2.1).
 *
 * A reconnection is not an attribute write — it goes through its own
 * command, and there is no schema key to address — but everything around
 * it is identical: persist, mark the results stale, capture the inverse.
 * Written as a second hook rather than a branch inside the first so
 * neither has to ask which of the two it is doing.
 *
 * `previous` is the pair the row was showing. Both ends travel together,
 * so the inverse is that pair and nothing has to be read back.
 */
export function useElementEndsWrite(): (
  elementId: string,
  fromId: string,
  toId: string,
  previous?: readonly [string, string],
) => Promise<void> {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();
  const { markEdited } = useNetworkVersion();

  return useCallback(
    async (elementId, fromId, toId, previous) => {
      if (!activeProjectId) return;
      try {
        await setElementEnds(activeProjectId, elementId, fromId, toId);
      } catch (err) {
        showToast(
          typeof err === "string" ? err : `Could not reconnect ${elementId}`,
          "error",
        );
        throw err;
      }
      if (previous) {
        pushUndoEntry(stackKey(activeProjectId, activeScenarioId ?? null), {
          label: `Reconnected ${elementId}`,
          undo: {
            ops: [
              {
                op: "reconnect",
                id: elementId,
                fromId: previous[0],
                toId: previous[1],
              },
            ],
          },
          redo: { ops: [{ op: "reconnect", id: elementId, fromId, toId }] },
        });
      }
      await saveProjectOnDisk(activeProjectId, activeScenarioId);
      markEdited(activeProjectId, activeScenarioId);
    },
    [activeProjectId, activeScenarioId, markEdited, showToast],
  );
}

/**
 * The same flow for a collection element's contents (§4.5.2.2).
 *
 * A third hook rather than a branch inside the others, for the same
 * reason there are two already: none of the three has to ask which of
 * them it is doing. What surrounds the write is identical — persist,
 * mark the results stale, capture the inverse.
 *
 * `previous` is the table the panel was showing, and it is the whole
 * inverse: the write replaces every row, so restoring it needs nothing
 * read back.
 */
export function useCollectionContentsWrite(): (
  kind: string,
  elementId: string,
  rows: number[][],
  previous?: number[][],
) => Promise<void> {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();
  const { markEdited } = useNetworkVersion();

  return useCallback(
    async (kind, elementId, rows, previous) => {
      if (!activeProjectId) return;
      try {
        await setCollectionContents(activeProjectId, kind, elementId, rows);
      } catch (err) {
        // Rethrown without a toast: the panel shows the reason beside
        // the table it is about, and "a curve's first column has to
        // increase" says nothing useful floating in a corner.
        showToast(typeof err === "string" ? err : String(err), "error");
        throw err;
      }
      if (previous) {
        pushUndoEntry(stackKey(activeProjectId, activeScenarioId ?? null), {
          label: `Edited ${elementId}`,
          undo: {
            ops: [{ op: "contents", kind, id: elementId, rows: previous }],
          },
          redo: { ops: [{ op: "contents", kind, id: elementId, rows }] },
        });
      }
      await saveProjectOnDisk(activeProjectId, activeScenarioId);
      markEdited(activeProjectId, activeScenarioId);
    },
    [activeProjectId, activeScenarioId, markEdited, showToast],
  );
}

/**
 * The same flow for a set of records attached to an element (§4.5.2.3).
 *
 * The fourth of these, and the last: every editable thing in the model
 * now goes through one of them, so none can ship with only the command
 * and none has to ask which of the four it is.
 *
 * `previous` is the set the panel was showing, and it is the whole
 * inverse — the write replaces every row.
 */
export function useElementRecordsWrite(): (
  elementId: string,
  set: string,
  rows: RecordSet["rows"],
  previous?: RecordSet["rows"],
) => Promise<void> {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();
  const { markEdited } = useNetworkVersion();

  return useCallback(
    async (elementId, set, rows, previous) => {
      if (!activeProjectId) return;
      try {
        await setElementRecords(activeProjectId, elementId, set, rows);
      } catch (err) {
        showToast(typeof err === "string" ? err : String(err), "error");
        throw err;
      }
      if (previous) {
        pushUndoEntry(stackKey(activeProjectId, activeScenarioId ?? null), {
          label: `Edited ${elementId}`,
          undo: {
            ops: [{ op: "records", id: elementId, set, rows: previous }],
          },
          redo: { ops: [{ op: "records", id: elementId, set, rows }] },
        });
      }
      await saveProjectOnDisk(activeProjectId, activeScenarioId);
      markEdited(activeProjectId, activeScenarioId);
    },
    [activeProjectId, activeScenarioId, markEdited, showToast],
  );
}
