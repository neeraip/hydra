// ── The records attached to an element ───────────────────────────────────────
//
// hydra-common §4.5.2.3: rows that belong to an element and have no
// identity of their own — a junction's demand categories, a vertex's
// dry-weather inflows.
//
// It renders nothing of its own. A record's cells are the same kinds of
// value an attribute holds, and the engine describes them with the same
// fields, so every cell here is the `AttributeField` the Properties rows
// and the Editor's tables already use. That reuse is the point of the
// section: the alternative was a second description of the same thing,
// and two descriptions of one thing drift.
//
// It exists because a junction with two demand categories used to read as
// one. The attribute schema could publish only their *sum* and the
// *first* one's pattern, so the second was invisible, and the write
// refused rather than distribute a total across categories nobody had
// described.

import { PlusIcon, TrashIcon } from "@heroicons/react/16/solid";
import { useCallback, useEffect, useId, useMemo, useState } from "react";
import { useActiveProject, useAppState } from "../../../AppContext";
import {
  getElementRecords,
  type RecordSet,
  useReferenceIds,
} from "../../../hooks";
import { formatIpcError } from "../../../hooks/ipc";
import { useNetworkVersion } from "../../../hooks/NetworkVersionContext";
import { useElementRecordsWrite } from "../../../hooks/useAttributeWrite";
import { compareNatural } from "../../../naturalOrder";
import { useUnitSystem } from "../../../units";
import { SectionLabel } from "../../ui/SectionLabel";
import { AttributeField } from "../attributeField";
import { cellEditor } from "../cellEditor";
import { ActionIcon, offerDatalist } from "../editorTable";
import { blankRecord, canAddRecord } from "./recordRow";

/**
 * Fetch the record sets attached to one element.
 *
 * Refetched after a write rather than patched in place, for the reason
 * every other surface here refetches: the engine is what knows what the
 * set became — it may have converted, reordered, or refused.
 */
export function useElementRecords(
  elementId: string,
  /** Which kind the id belongs to. Every water-distribution record set
   * hangs off a node, so without this a pipe `10` was served junction
   * `10`'s demand categories — and that set is editable. */
  kind?: string,
): {
  sets: RecordSet[];
  refetch: () => void;
} {
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();
  // Keyed on the model's version as well: this panel refetches after its
  // own write, and an undo is a change it did not make — so the records
  // it drew went on being the ones that had just been put back.
  const { version } = useNetworkVersion();
  const [sets, setSets] = useState<RecordSet[]>([]);
  // Fetched directly rather than through a counter that exists only to
  // re-run the effect — the same choice `useElementDetails` makes, and
  // for the same reason: a dependency that is not a dependency reads as
  // one, and the next person removes it.
  const refetch = useCallback(() => {
    if (!project?.id || !elementId) return;
    void getElementRecords(project.id, activeScenarioId, elementId, kind).then(
      setSets,
    );
  }, [project?.id, activeScenarioId, elementId, kind]);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the version is the intentional refetch trigger
  useEffect(() => {
    if (!project?.id || !elementId) {
      setSets([]);
      return;
    }
    let cancelled = false;
    void getElementRecords(project.id, activeScenarioId, elementId, kind).then(
      (r) => {
        if (!cancelled) setSets(r);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [project?.id, activeScenarioId, elementId, kind, version]);
  return { sets, refetch };
}

/**
 * The sets worth drawing: the ones that hold something, plus the empty
 * ones that can be added to.
 *
 * An empty set is not always nothing. A junction with no demand
 * categories still shows its table, because the row of headings and the
 * add button are how the first category is entered. A drainage node's
 * dry weather inflows are served read-only, so an empty one has nothing
 * to read and no way to get anything — and it drew a heading and a row
 * of column names under every node in every drainage model, which reads
 * as a panel that failed to load rather than as an element that has no
 * inflows.
 *
 * Keyed on `editable` rather than on the set's name, so it stays true of
 * whatever a future engine attaches: what decides it is whether the
 * empty table is an offer.
 */
export function shownRecordSets(sets: RecordSet[]): RecordSet[] {
  return sets.filter((set) => set.rows.length > 0 || set.editable);
}

export function RecordSets({
  elementId,
  kind,
  sets,
  onEdited,
}: {
  elementId: string;
  /** Which kind the element is — half its address, see
   * {@link useElementRecords}. */
  kind?: string;
  sets: RecordSet[];
  /** Called after a successful write, so the caller can refetch. */
  onEdited?: () => void;
}) {
  const shown = shownRecordSets(sets);
  if (shown.length === 0) return null;
  return (
    <>
      {shown.map((set) => (
        <RecordTable
          key={set.key}
          elementId={elementId}
          kind={kind}
          set={set}
          onEdited={onEdited}
        />
      ))}
    </>
  );
}

function RecordTable({
  elementId,
  kind,
  set,
  onEdited,
}: {
  elementId: string;
  kind?: string;
  set: RecordSet;
  onEdited?: () => void;
}) {
  const sys = useUnitSystem();
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();
  const write = useElementRecordsWrite();
  const [refused, setRefused] = useState<string | null>(null);
  const listPrefix = useId();

  // The ids the reference columns may name, for the kinds this set's
  // columns declare (§4.5.1.1) — usually one, often none.
  const referenced = useMemo(
    () => [...new Set(set.columns.flatMap((c) => c.references ?? []))],
    [set.columns],
  );
  const referenceIds = useReferenceIds(
    project?.id,
    activeScenarioId,
    referenced,
  );
  const lists = useMemo(() => {
    const out: Array<{ key: string; ids: string[] }> = [];
    for (const c of set.columns) {
      const ids = [
        ...new Set((c.references ?? []).flatMap((k) => referenceIds[k] ?? [])),
      ].sort(compareNatural);
      if (ids.length && offerDatalist(ids.length))
        out.push({ key: c.key, ids });
    }
    return out;
  }, [set.columns, referenceIds]);

  const send = (rows: RecordSet["rows"]) => {
    setRefused(null);
    return write(elementId, set.key, rows, {
      previous: set.rows,
      kind,
      label: set.label,
    })
      .then(() => onEdited?.())
      .catch((e: unknown) => {
        setRefused(formatIpcError(e));
      });
  };

  return (
    <>
      <SectionLabel>{set.label}</SectionLabel>
      {lists.map((l) => (
        <datalist key={l.key} id={`${listPrefix}-${l.key}`}>
          {l.ids.map((id) => (
            <option key={id} value={id} />
          ))}
        </datalist>
      ))}
      <table
        style={{
          width: "100%",
          borderCollapse: "collapse",
          // Tightened only where the add button sits under it.
          marginBottom: canAddRecord(set) ? 4 : 14,
        }}
      >
        <thead>
          <tr>
            {set.columns.map((c) => {
              const unit = c.quantity
                ? sys === "us"
                  ? c.quantity.usLabel
                  : c.quantity.siLabel
                : null;
              return (
                <th
                  key={c.key}
                  style={{
                    textAlign: "left",
                    padding: "2px 0",
                    fontSize: "var(--text-sm)",
                    fontWeight: 500,
                    color: "var(--text-tertiary)",
                  }}
                >
                  {unit ? `${c.label} (${unit})` : c.label}
                </th>
              );
            })}
            {set.editable && <th style={{ width: 1 }} />}
          </tr>
        </thead>
        <tbody>
          {set.rows.map((row, r) => (
            // Keyed by position: a record is addressed by where it sits
            // in its set and by nothing else, so two identical rows are
            // two records rather than one.
            // biome-ignore lint/suspicious/noArrayIndexKey: records are positional
            <tr key={r}>
              {set.columns.map((c, i) => {
                const value = row[i] ?? null;
                const editor = cellEditor(
                  { ...c, editable: set.editable, values: [] },
                  value,
                  set.editable,
                );
                return (
                  <td
                    key={c.key}
                    style={{
                      padding: editor.kind === "none" ? "3px 0" : 0,
                      fontFamily: "var(--font-mono)",
                      fontSize: "var(--text-md)",
                    }}
                  >
                    {editor.kind === "none" ? (
                      (value ?? "—")
                    ) : (
                      <AttributeField
                        editor={editor}
                        quantity={c.quantity}
                        sys={sys}
                        label={`${elementId} ${set.label} ${r + 1} ${c.label}`}
                        listId={
                          lists.some((l) => l.key === c.key)
                            ? `${listPrefix}-${c.key}`
                            : undefined
                        }
                        onCommit={(next) =>
                          send(
                            set.rows.map((other, or) =>
                              or === r
                                ? other.map((cell, oc) =>
                                    oc === i ? next : cell,
                                  )
                                : other,
                            ),
                          )
                        }
                      />
                    )}
                  </td>
                );
              })}
              {set.editable && (
                <td style={{ padding: "0 4px" }}>
                  <ActionIcon
                    title="Remove record"
                    danger
                    onClick={() => send(set.rows.filter((_, or) => or !== r))}
                  >
                    <TrashIcon style={{ width: 13, height: 13 }} />
                  </ActionIcon>
                </td>
              )}
            </tr>
          ))}
        </tbody>
      </table>

      {/* A new record is the set with a row more — the same write, which
          is why there is no separate "add" operation to refuse. Offered
          only where there is room for one, and holding what each column
          can actually take: see `recordRow`. */}
      {canAddRecord(set) && (
        <div style={{ marginBottom: 14 }}>
          <ActionIcon
            title="Add record"
            onClick={() => send([...set.rows, blankRecord(set)])}
          >
            <PlusIcon style={{ width: 13, height: 13 }} />
          </ActionIcon>
        </div>
      )}

      {/* Beside the table it is about, for the same reason the contents
          panel shows one there: a refusal that floats away takes its
          reason with it. */}
      {refused && (
        <div
          style={{
            marginBottom: 14,
            fontSize: "var(--text-sm)",
            color: "var(--danger)",
          }}
        >
          {refused}
        </div>
      )}
    </>
  );
}
