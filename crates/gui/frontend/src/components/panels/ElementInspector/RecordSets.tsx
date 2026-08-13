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
import { useElementRecordsWrite } from "../../../hooks/useAttributeWrite";
import { useUnitSystem } from "../../../units";
import { SectionLabel } from "../../ui/SectionLabel";
import { AttributeField } from "../attributeField";
import { cellEditor } from "../cellEditor";
import { ActionIcon, offerDatalist } from "../editorTable";

/**
 * Fetch the record sets attached to one element.
 *
 * Refetched after a write rather than patched in place, for the reason
 * every other surface here refetches: the engine is what knows what the
 * set became — it may have converted, reordered, or refused.
 */
export function useElementRecords(elementId: string): {
  sets: RecordSet[];
  refetch: () => void;
} {
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();
  const [sets, setSets] = useState<RecordSet[]>([]);
  // Fetched directly rather than through a counter that exists only to
  // re-run the effect — the same choice `useElementDetails` makes, and
  // for the same reason: a dependency that is not a dependency reads as
  // one, and the next person removes it.
  const refetch = useCallback(() => {
    if (!project?.id || !elementId) return;
    void getElementRecords(project.id, activeScenarioId, elementId).then(
      setSets,
    );
  }, [project?.id, activeScenarioId, elementId]);
  useEffect(() => {
    if (!project?.id || !elementId) {
      setSets([]);
      return;
    }
    let cancelled = false;
    void getElementRecords(project.id, activeScenarioId, elementId).then(
      (r) => {
        if (!cancelled) setSets(r);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [project?.id, activeScenarioId, elementId]);
  return { sets, refetch };
}

export function RecordSets({
  elementId,
  sets,
  onEdited,
}: {
  elementId: string;
  sets: RecordSet[];
  /** Called after a successful write, so the caller can refetch. */
  onEdited?: () => void;
}) {
  if (sets.length === 0) return null;
  return (
    <>
      {sets.map((set) => (
        <RecordTable
          key={set.key}
          elementId={elementId}
          set={set}
          onEdited={onEdited}
        />
      ))}
    </>
  );
}

function RecordTable({
  elementId,
  set,
  onEdited,
}: {
  elementId: string;
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
      ].sort();
      if (ids.length && offerDatalist(ids.length))
        out.push({ key: c.key, ids });
    }
    return out;
  }, [set.columns, referenceIds]);

  const send = (rows: RecordSet["rows"]) => {
    setRefused(null);
    return write(elementId, set.key, rows, set.rows)
      .then(() => onEdited?.())
      .catch((e: unknown) => {
        setRefused(typeof e === "string" ? e : String(e));
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
          marginBottom: set.editable ? 4 : 14,
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
          is why there is no separate "add" operation to refuse. */}
      {set.editable && (
        <div style={{ marginBottom: 14 }}>
          <ActionIcon
            title="Add record"
            onClick={() =>
              send([
                ...set.rows,
                set.columns.map((c) => (c.kind?.type === "number" ? 0 : "")),
              ])
            }
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
