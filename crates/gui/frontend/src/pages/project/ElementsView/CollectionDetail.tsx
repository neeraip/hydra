/**
 * The contents of one container element (hydra-common §4.5.2.2).
 *
 * A collection row can only report what a thing *is* and how big — a curve
 * with 14 points, a rule with 3 clauses — because the contents do not fit
 * a table cell. This is where they are actually shown, and edited.
 *
 * Two renderings, chosen by what the engine returned rather than by the
 * kind asked for: a table of numbers under engine-named headings, or
 * verbatim lines for containers whose content is language.
 *
 * Only the first takes an edit, and the engine says which it served. A
 * rule is language, and rewriting it means parsing that language with the
 * engine's own model reader — so it is shown to be read.
 *
 * **Every change sends the whole table.** The rows are ordered and
 * interdependent: a curve's abscissae must ascend, a pattern's
 * multipliers are a cycle whose length is its period. A row added before
 * it is sorted into place is a curve that is briefly illegal, so there is
 * no per-row write to refuse it — one table, one validation, and the
 * inverse is the table that was there.
 */

import { PlusIcon, TrashIcon } from "@heroicons/react/16/solid";
import { useState } from "react";
import { ActionIcon } from "../../../components/panels/editorTable";
import { EditableNumber } from "../../../components/ui/EditableNumber";
import {
  type CollectionDetail as Detail,
  formatGenericValue,
  genericUnitLabel,
} from "../../../hooks";
import { formatIpcError } from "../../../hooks/ipc";
import { useUnitSystem } from "../../../units";
import { nextRow } from "./nextRow";

/**
 * Whether the panel has anything to draw.
 *
 * Not every collection kind has contents. A pollutant, a land use and an
 * aquifer are their attributes and nothing further, and a control
 * measure's layers live in the records panel. All six reached this panel and it drew
 * itself anyway, under a sentence written for one other case entirely —
 * "this entry's contents are held outside the model file", which is true
 * of an external time series and false of every one of them.
 *
 * So the engine says why an empty result is empty (§4.5.2.2) and this
 * asks whether there is anything: rows, lines, a note, or an empty table
 * that can be added to. The last is the reason the records panel keeps
 * an empty editable set — the headings and the add button are how the
 * first row is entered, so that table is an offer rather than a blank.
 */
function hasContentsToShow(detail: Detail, canWrite: boolean): boolean {
  return (
    detail.rows.length > 0 ||
    detail.lines.length > 0 ||
    !!detail.note ||
    (canWrite && detail.editable)
  );
}

export function CollectionDetail({
  detail,
  elementId,
  onWrite,
}: {
  detail: Detail;
  /** The container being shown, for the panel's heading. */
  elementId: string;
  /** Replace the contents. Absent, or contents the engine did not mark
   * editable, and the table reads only. */
  onWrite?: (rows: number[][]) => Promise<void> | void;
}) {
  const sys = useUnitSystem();
  const hasTable = detail.rows.length > 0;
  const hasLines = detail.lines.length > 0;
  const editable = !!onWrite && detail.editable;
  // What the last write was refused for. Held here rather than toasted
  // because the reason is about this table — "a curve's first column has
  // to increase" belongs beside the column it is about.
  const [refused, setRefused] = useState<string | null>(null);

  // Handled here and not rethrown. Every caller is an event handler that
  // drops the promise, so rethrowing reached nobody and surfaced as an
  // unhandled rejection — which failed the whole suite while every test
  // in it passed. Showing the reason beside the table *is* what handling
  // a refusal means here, the same shape `RecordSets` uses.
  const send = (rows: number[][]) => {
    setRefused(null);
    return Promise.resolve(onWrite?.(rows)).catch((e: unknown) => {
      setRefused(formatIpcError(e));
    });
  };

  // Absent rather than empty, for the reason the records strip below is:
  // both are bordered boxes with their own background, so one holding
  // only a heading reads as something that failed rather than as a kind
  // with no contents.
  if (!hasContentsToShow(detail, !!onWrite)) return null;

  return (
    <div
      style={{
        borderTop: "1px solid var(--border)",
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
        maxHeight: "45%",
        background: "var(--bg-panel)",
      }}
    >
      <div
        style={{
          padding: "6px 12px",
          fontSize: "var(--text-sm)",
          fontWeight: 600,
          color: "var(--text-secondary)",
          fontFamily: "var(--font-ui)",
          flexShrink: 0,
        }}
      >
        {elementId}
      </div>

      <div style={{ overflow: "auto", flex: 1, minHeight: 0 }}>
        {hasTable && (
          <table
            style={{
              width: "100%",
              borderCollapse: "collapse",
              fontFamily: "var(--font-mono)",
              fontSize: "var(--text-md)",
            }}
          >
            <thead>
              <tr>
                {detail.columns.map((c, i) => {
                  const unit = genericUnitLabel(
                    detail.quantities[i] ?? undefined,
                    sys,
                  );
                  return (
                    <th
                      key={c}
                      style={{
                        textAlign: "left",
                        padding: "4px 12px",
                        color: "var(--text-tertiary)",
                        fontWeight: 500,
                        fontFamily: "var(--font-ui)",
                        borderBottom: "1px solid var(--border)",
                        position: "sticky",
                        top: 0,
                        background: "var(--bg-panel)",
                      }}
                    >
                      {unit ? `${c} (${unit})` : c}
                    </th>
                  );
                })}
              </tr>
            </thead>
            <tbody>
              {detail.rows.map((row, r) => (
                // Keyed by position, not by content: rows are positional
                // here — two identical points are two points — and a key
                // built from the values would collapse them into one.
                // biome-ignore lint/suspicious/noArrayIndexKey: rows are positional
                <tr key={r}>
                  {row.map((v, i) => (
                    <td
                      key={detail.columns[i] ?? i}
                      style={{
                        padding: editable ? 0 : "3px 12px",
                        borderBottom: "1px solid rgba(255,255,255,0.04)",
                      }}
                    >
                      {/* Values arrive SI; the column's quantity is what
                          makes them displayable in the user's system, and
                          what converts back what was typed. */}
                      {editable ? (
                        <EditableNumber
                          value={v}
                          quantity={detail.quantities[i] ?? undefined}
                          sys={sys}
                          chrome="cell"
                          label={`${elementId} row ${r + 1} ${detail.columns[i] ?? i + 1}`}
                          onCommit={(next) =>
                            send(
                              detail.rows.map((other, or) =>
                                or === r
                                  ? other.map((cell, oc) =>
                                      oc === i ? next : cell,
                                    )
                                  : other,
                              ),
                            )
                          }
                        />
                      ) : (
                        formatGenericValue(
                          v,
                          detail.quantities[i] ?? undefined,
                          sys,
                          false,
                        )
                      )}
                    </td>
                  ))}
                  {editable && (
                    <td style={{ padding: "0 8px", width: 1 }}>
                      <ActionIcon
                        title="Remove row"
                        danger
                        onClick={() =>
                          send(detail.rows.filter((_, or) => or !== r))
                        }
                      >
                        <TrashIcon style={{ width: 13, height: 13 }} />
                      </ActionIcon>
                    </td>
                  )}
                </tr>
              ))}
            </tbody>
          </table>
        )}

        {/* Adding a row is the one edit a cell cannot express. It lands
            at the end, seeded past the last row in the column the engine
            says must advance — a row of zeros under a curve or a series
            was one the write could only refuse, so the button always
            failed. */}
        {editable && (
          <div style={{ padding: "6px 12px" }}>
            <ActionIcon
              title="Add row"
              onClick={() =>
                send([
                  ...detail.rows,
                  nextRow(detail.rows, detail.columns.length, detail.advances),
                ])
              }
            >
              <PlusIcon style={{ width: 13, height: 13 }} />
            </ActionIcon>
          </div>
        )}

        {/* Beside the table rather than in a toast: "a curve's first
            column has to increase" is about the column above it, and a
            notification that slides away takes the reason with it. */}
        {refused && (
          <div
            style={{
              padding: "6px 12px",
              fontSize: "var(--text-sm)",
              color: "var(--danger)",
            }}
          >
            {refused}
          </div>
        )}

        {hasLines && (
          <pre
            style={{
              margin: 0,
              padding: "6px 12px",
              fontFamily: "var(--font-mono)",
              fontSize: "var(--text-md)",
              color: "var(--text-secondary)",
              whiteSpace: "pre-wrap",
            }}
          >
            {detail.lines.join("\n")}
          </pre>
        )}

        {/* The engine's sentence, not ours. It used to be written here,
            which meant one case's explanation was shown for every empty
            result — an external series' values really are held in a file
            beside the model, and a pollutant's really are nowhere. */}
        {!hasTable && !hasLines && detail.note && (
          <div
            style={{
              padding: "10px 12px",
              fontSize: "var(--text-md)",
              color: "var(--text-tertiary)",
            }}
          >
            {detail.note}
          </div>
        )}
      </div>
    </div>
  );
}
