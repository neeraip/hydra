import { useCallback } from "react";
import { useAppState } from "../AppContext";
import { formatIpcError } from "./ipc";
import { useNetworkVersion } from "./NetworkVersionContext";
import {
  deleteElement,
  patchNodePosition,
  type RecordSet,
  type Removed,
  setCollectionContents,
  setElementAttribute,
  setElementEnds,
  setElementRecords,
} from "./network";
import { persistOrSay } from "./projects";
import { moveEntry, pushUndoEntry, stackKey } from "./undoStack";

/**
 * Shared attribute-write flow for every place a model number is edited —
 * the element inspector's Properties rows and the Editor's per-kind
 * tables.
 *
 * A write is four things, and only the first is the command:
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
 * The same four that `useElementRename` does. Every editable thing in
 * the model goes through one of the hooks in this file, so a second
 * editing surface cannot ship with only the first of them — which is
 * exactly how the inspector shipped, and later how the Editor's move,
 * delete and add each shipped in turn.
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
  /** Which kind the id belongs to. An id is a whole address in the
   * drainage engine and half of one in water distribution, where a
   * junction `10` and a pipe `10` are two elements — so a caller that
   * knows says which, and the undo carries it too. */
  kind?: string,
) => Promise<void> {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();
  const { markEdited } = useNetworkVersion();

  return useCallback(
    async (elementId, key, value, previous, kind) => {
      if (!activeProjectId) return;
      try {
        await setElementAttribute(activeProjectId, elementId, key, value, kind);
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
          subject: kind ? { kind, id: elementId } : undefined,
          undo: {
            ops: [{ op: "set", id: elementId, key, value: previous, kind }],
          },
          redo: { ops: [{ op: "set", id: elementId, key, value, kind }] },
        });
      }
      await persistOrSay(activeProjectId, activeScenarioId, showToast);
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
  /** Which kind the element is, so the history can show its badge. */
  kind?: string,
) => Promise<void> {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();
  const { markEdited } = useNetworkVersion();

  return useCallback(
    async (elementId, fromId, toId, previous, kind) => {
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
          subject: kind ? { kind, id: elementId } : undefined,
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
      await persistOrSay(activeProjectId, activeScenarioId, showToast);
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
        showToast(formatIpcError(err), "error");
        throw err;
      }
      if (previous) {
        pushUndoEntry(stackKey(activeProjectId, activeScenarioId ?? null), {
          label: `Edited ${elementId}`,
          subject: { kind, id: elementId },
          undo: {
            ops: [{ op: "contents", kind, id: elementId, rows: previous }],
          },
          redo: { ops: [{ op: "contents", kind, id: elementId, rows }] },
        });
      }
      await persistOrSay(activeProjectId, activeScenarioId, showToast);
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
 *
 * The trailing three travel in one object rather than as three optional
 * positions: `kind` and `label` are both an optional string and sat
 * beside each other, which is a swap nobody would see at the call and
 * nothing would catch.
 */
export interface RecordWriteContext {
  /** The set the panel was showing, and the whole inverse of the write. */
  previous?: RecordSet["rows"];
  /** Which kind the id belongs to — half its address in water
   * distribution, where every record set hangs off a node and a pipe may
   * share a junction's id. Carried into the undo for the same reason. */
  kind?: string;
  /** What the engine calls the set. An element may carry several — a
   * control measure carries six — so without it the history read as six
   * identical entries against one id. */
  label?: string;
}

/**
 * What one record-set write is called in the history.
 *
 * Named by the set as well as the element, because an element may carry
 * several: a control measure carries six layers, and six entries all
 * reading "Edited GR1" are a history nobody can undo *to* a point. The
 * set's name is lowercased into the sentence — the engine capitalises it
 * as a heading ("Snow surfaces"), and a heading dropped mid-sentence
 * reads as a proper noun.
 *
 * An element with one set says only its id, which is every water
 * distribution element and most drainage ones.
 */
export function recordEntryLabel(elementId: string, setLabel?: string): string {
  if (!setLabel) return `Edited ${elementId}`;
  return `Edited ${elementId} ${setLabel.toLowerCase()}`;
}

export function useElementRecordsWrite(): (
  elementId: string,
  set: string,
  rows: RecordSet["rows"],
  context?: RecordWriteContext,
) => Promise<void> {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();
  const { markEdited } = useNetworkVersion();

  return useCallback(
    async (elementId, set, rows, context) => {
      if (!activeProjectId) return;
      const { previous, kind, label } = context ?? {};
      try {
        await setElementRecords(activeProjectId, elementId, set, rows, kind);
      } catch (err) {
        showToast(formatIpcError(err), "error");
        throw err;
      }
      if (previous) {
        pushUndoEntry(stackKey(activeProjectId, activeScenarioId ?? null), {
          label: recordEntryLabel(elementId, label),
          subject: kind ? { kind, id: elementId } : undefined,
          undo: {
            ops: [{ op: "records", id: elementId, set, rows: previous, kind }],
          },
          redo: { ops: [{ op: "records", id: elementId, set, rows, kind }] },
        });
      }
      await persistOrSay(activeProjectId, activeScenarioId, showToast);
      markEdited(activeProjectId, activeScenarioId);
    },
    [activeProjectId, activeScenarioId, markEdited, showToast],
  );
}

/**
 * Moving one element, everywhere a position is set.
 *
 * Dragged on the canvas, typed into the Editor's X and Y columns. The
 * canvas did all four things in its own callback and the Editor did the
 * command and the undo capture only — so a position typed into the table
 * was gone at the next open, and the results beside it went on looking
 * current. `moveEntry` already existed to stop exactly this drift in the
 * *history*, and the drift moved to the two steps it did not cover.
 *
 * `before` is where the element was, read before the patch. Without it
 * the move still happens and is simply not captured, which is what
 * `moveEntry` has always said about an inverse nobody can supply.
 */
export function useElementMoveWrite(): (
  elementId: string,
  before: readonly [number, number] | null | undefined,
  x: number,
  y: number,
  kind?: string,
) => Promise<void> {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();
  const { markEdited } = useNetworkVersion();

  return useCallback(
    async (elementId, before, x, y, kind) => {
      if (!activeProjectId) return;
      try {
        await patchNodePosition(elementId, x, y);
      } catch (err) {
        showToast(formatIpcError(err), "error");
        throw err;
      }
      const entry = moveEntry(elementId, before, x, y, kind);
      if (entry) {
        pushUndoEntry(
          stackKey(activeProjectId, activeScenarioId ?? null),
          entry,
        );
      }
      await persistOrSay(activeProjectId, activeScenarioId, showToast);
      markEdited(activeProjectId, activeScenarioId);
    },
    [activeProjectId, activeScenarioId, markEdited, showToast],
  );
}

/**
 * Removing one element, everywhere one is deleted.
 *
 * The history is deliberately *not* handled here, and that is the one
 * part of a removal the two surfaces genuinely differ on: the canvas
 * reads a snapshot first and can offer recreate specs, and the Editor
 * cannot, so one captures an entry and the other clears the stack. What
 * they do not differ on is that the model has changed on disk and the
 * results have stopped describing it — which the Editor's delete did
 * neither of.
 *
 * Returns what the engine says went with it, so the caller can report a
 * cascade it did not ask for.
 */
export function useElementRemoveWrite(): (
  kind: string,
  elementId: string,
) => Promise<Removed> {
  const { activeProjectId, activeScenarioId, showToast } = useAppState();
  const { markEdited } = useNetworkVersion();

  return useCallback(
    async (kind, elementId) => {
      const removed = await deleteElement(kind, elementId);
      if (activeProjectId) {
        await persistOrSay(activeProjectId, activeScenarioId, showToast);
        markEdited(activeProjectId, activeScenarioId);
      }
      return removed;
    },
    [activeProjectId, activeScenarioId, markEdited, showToast],
  );
}
