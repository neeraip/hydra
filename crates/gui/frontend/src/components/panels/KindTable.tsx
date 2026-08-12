// ── The per-kind element table ────────────────────────────────────────────────
//
// One kind, one table, all of its own columns — the arrangement a shared
// Nodes/Links table cannot offer, because columns common to junctions,
// outfalls and storage units are barely any columns at all.
//
// Properties only: what the model file declares. Results are deliberately
// absent. They belong to a moment in a run, and the page this table lives on
// has no timeline to choose that moment with — a column headed "current"
// beside a scrub bar that does not exist answers a question nobody asked.
// Results live where the timeline does: the canvas rail and the element
// inspector.
//
// Columns are engine-authored — labels, units and ordering come from the
// engine's §4.4 attribute schema — so a kind this file has never heard of
// renders correctly, and so does an engine that does not exist yet.
//
// Which cells take an input is engine-authored too: each column carries
// the backend's own answer to whether that attribute can be written, and
// this file never asks which engine it is drawing. A column that cannot
// be written renders exactly as it did before editing existed.
//
// Everything about how it *looks and scrolls* comes from `editorTable`,
// which the water-distribution tables use too. This file had its own row
// metrics, its own header styling and no virtualisation, and the two
// tables had visibly drifted: uppercase headers, rows four pixels
// shorter, separators too faint to read as a grid, no hover, and every
// row of a several-thousand-conduit model mounted at once.

import {
  MapPinIcon,
  PencilSquareIcon,
  TrashIcon,
} from "@heroicons/react/16/solid";
import { useEffect, useMemo, useRef, useState } from "react";
import type { KindElements } from "../../hooks";
import { formatElementAttribute } from "../../hooks/network";
import { readTextScale } from "../../textScale";
import { useUnitSystem } from "../../units";
import { EditableNumber } from "../ui/EditableNumber";
import { cellEditor } from "./cellEditor";
import {
  ActionIcon,
  ActionsTh,
  EDITOR_TD,
  editorRowHeight,
  editorRowHover,
  editorRowStyle,
  RowActionsCell,
  SortTh,
  useVirtualRows,
  VirtualSpacerRow,
} from "./editorTable";

type SortDir = "asc" | "desc";

/**
 * One element kind's table.
 *
 * Carries no kind column: the table is already scoped to a single kind by
 * the Editor's rail, so a badge on every row would repeat the same glyph
 * down the page and buy a column's width for nothing.
 *
 * Mount one per kind (`key={kind}`) — the sort column belongs to the kind
 * being shown, and carrying it across would leave a table sorted by a
 * column it does not have.
 */
export function KindTable({
  elements,
  activeId,
  onSelect,
  onEdit,
  onMove,
  onReveal,
  onRename,
  onDelete,
  revealToken,
}: {
  /** §4.4 property columns for this kind. */
  elements: KindElements;
  activeId?: string | null;
  onSelect?: (id: string) => void;
  /**
   * Write one cell back, in the unit the column serves.
   *
   * Absent means the table reads only — which is what an engine that
   * declares no writable attributes gets, without this file knowing
   * which engine that is. The columns say what may be written; this
   * prop says whether anything is listening.
   */
  onEdit?: (
    id: string,
    key: string,
    value: number | string,
    /** What the cell was showing, so the write can be undone. */
    previous: number | string,
  ) => Promise<void> | void;
  /**
   * Move one element, in the model's own coordinate system.
   *
   * Separate from `onEdit` because a position is not an attribute
   * (hydra-common §4.5.2): it is implied by the element's class, has no
   * schema key to be addressed by, and is written by its own operation.
   * Absent, or a kind that is not anywhere, and the columns do not
   * appear.
   */
  onMove?: (id: string, x: number, y: number) => Promise<void> | void;
  /**
   * Per-row actions, each offered only when its handler is given.
   *
   * A table of elements is where a reader finds one, so the things they
   * then want to do to it belong on the row rather than behind a
   * selection and a trip elsewhere. An engine that cannot rename shows
   * no rename; the column disappears entirely when none are given.
   */
  onReveal?: (id: string) => void;
  onRename?: (id: string) => void;
  onDelete?: (id: string) => void;
  /**
   * Bumped by the caller to mean "bring `activeId` into view now".
   *
   * A token rather than a boolean because the same element can be revealed
   * twice in a row — asking for J5 again after scrolling away has to move
   * the table again, and a boolean that is already `true` cannot say so.
   */
  revealToken?: number;
}) {
  const sys = useUnitSystem();
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [query, setQuery] = useState("");
  const scrollRef = useRef<HTMLDivElement | null>(null);

  function toggleSort(col: string) {
    if (sortCol !== col) {
      setSortCol(col);
      setSortDir("asc");
    } else if (sortDir === "asc") {
      setSortDir("desc");
    } else {
      setSortCol(null);
      setSortDir("asc");
    }
  }

  // Matching is on the id alone, not every column.
  //
  // A drainage model has thousands of conduits, and the question a search
  // box answers here is "where is C1423?" — searching the property values
  // as well would return rows whose diameter happens to contain the digits
  // typed, burying the one that was asked for.
  const matches = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return null;
    return new Set(
      elements.ids
        .map((id, i) => (id.toLowerCase().includes(q) ? i : -1))
        .filter((i) => i >= 0),
    );
  }, [elements.ids, query]);

  // Rows are indices into the columnar arrays, so sorting never copies the
  // values themselves.
  const order = useMemo(() => {
    const idx = elements.ids
      .map((_, i) => i)
      .filter((i) => matches == null || matches.has(i));
    if (!sortCol) return idx;
    const propCol = elements.columns.find((c) => c.key === sortCol);
    const axis = sortCol === "x" ? 0 : sortCol === "y" ? 1 : null;
    const get = (i: number): number | string | null => {
      if (propCol) return propCol.values[i] ?? null;
      if (axis != null) return elements.positions[i]?.[axis] ?? null;
      return elements.ids[i];
    };
    return idx.sort((a, b) => {
      const av = get(a) ?? "";
      const bv = get(b) ?? "";
      const cmp = av < bv ? -1 : av > bv ? 1 : 0;
      return sortDir === "asc" ? cmp : -cmp;
    });
  }, [elements, sortCol, sortDir, matches]);

  const { virtualItems, paddingTop, paddingBottom } = useVirtualRows(
    order,
    scrollRef,
  );

  // What the reveal below needs, read through refs rather than listed as
  // dependencies. It has to run when the caller *asks* — the token — and
  // not every time the sort order or the selection happens to change,
  // which would yank the table back mid-scroll.
  const revealTarget = useRef({ activeId, order, ids: elements.ids });
  revealTarget.current = { activeId, order, ids: elements.ids };

  // Reveal: clear any search first, because a filter that excludes the
  // requested element would leave the table looking empty in response to
  // "show me this element" — then scroll to the row.
  //
  // Scrolled by arithmetic rather than by `scrollIntoView`: the row is
  // very likely not mounted, which is the point of virtualising, and an
  // element that does not exist cannot be scrolled to.
  useEffect(() => {
    if (revealToken == null) return;
    setQuery("");
    const raf = requestAnimationFrame(() => {
      const container = scrollRef.current;
      const { activeId: id, order: rows, ids } = revealTarget.current;
      if (!container || id == null) return;
      const row = rows.findIndex((i) => ids[i] === id);
      if (row < 0) return;
      const rowHeight = editorRowHeight(readTextScale());
      container.scrollTop = Math.max(
        0,
        row * rowHeight - container.clientHeight / 2 + rowHeight / 2,
      );
    });
    return () => cancelAnimationFrame(raf);
  }, [revealToken]);

  // A column of numbers is read down its digits, so it is right-aligned
  // — the same rule the water-distribution tables apply, decided here
  // from the values rather than from a column name this file cannot
  // interpret. A quantity is not the test: a roughness is a number with
  // no unit, and a boundary condition is a word.
  const numeric = useMemo(
    () =>
      elements.columns.map((c) => c.values.some((v) => typeof v === "number")),
    [elements.columns],
  );

  // The model's own coordinate system, whatever that is: these numbers
  // are never converted on the way in, so they are never converted on
  // the way out, and they carry no quantity for the same reason. A
  // model may be a map or a drawing and this table cannot tell.
  const placed = elements.positions.length === elements.ids.length;
  const hasActions = !!(onReveal || onRename || onDelete);
  const columnCount =
    elements.columns.length + 1 + (placed ? 2 : 0) + (hasActions ? 1 : 0);

  if (elements.ids.length === 0) {
    return (
      <div
        style={{
          padding: 16,
          fontSize: "var(--text-md)",
          color: "var(--text-tertiary)",
        }}
      >
        No elements of this kind.
      </div>
    );
  }

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
      }}
    >
      {/* The same bar the water-distribution editor puts above its
          tables: 44px tall, search right-aligned in it. That editor also
          has an Add button here; drainage adds elements on the map,
          where a new one needs somewhere to go. */}
      <div
        style={{
          height: 44,
          display: "flex",
          alignItems: "center",
          justifyContent: "flex-end",
          padding: "0 12px",
          borderBottom: "1px solid var(--border)",
          background: "var(--bg-panel)",
          flexShrink: 0,
        }}
      >
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search…"
          aria-label="Search ids"
          style={{
            width: 200,
            height: 28,
            background: "var(--bg-input)",
            border: "1px solid var(--border)",
            borderRadius: 5,
            padding: "0 8px",
            color: "var(--text-primary)",
            fontFamily: "var(--font-ui)",
            fontSize: "var(--text-lg)",
            outline: "none",
          }}
        />
      </div>
      {/* A search that matches nothing is not the same as a kind with
          nothing in it, and saying "no elements of this kind" here would
          be false. */}
      {order.length === 0 ? (
        <div
          style={{
            padding: 16,
            fontSize: "var(--text-md)",
            color: "var(--text-tertiary)",
          }}
        >
          No ids match “{query}”.
        </div>
      ) : (
        <div ref={scrollRef} style={{ overflow: "auto", flex: 1 }}>
          <table
            style={{
              width: "100%",
              borderCollapse: "collapse",
              fontSize: "var(--text-lg)",
            }}
          >
            <thead>
              <tr>
                <SortTh
                  field="id"
                  label="ID"
                  sortField={sortCol}
                  sortAsc={sortDir === "asc"}
                  onSort={toggleSort}
                  markUnsorted
                />
                {placed &&
                  (["x", "y"] as const).map((axis) => (
                    <SortTh
                      key={axis}
                      field={axis}
                      label={axis.toUpperCase()}
                      sortField={sortCol}
                      sortAsc={sortDir === "asc"}
                      onSort={toggleSort}
                      align="right"
                      markUnsorted
                    />
                  ))}
                {elements.columns.map((c, ci) => (
                  <SortTh
                    key={c.key}
                    align={numeric[ci] ? "right" : "left"}
                    field={c.key}
                    label={
                      c.quantity
                        ? `${c.label} (${sys === "us" ? c.quantity.usLabel : c.quantity.siLabel})`
                        : c.label
                    }
                    sortField={sortCol}
                    sortAsc={sortDir === "asc"}
                    onSort={toggleSort}
                    markUnsorted
                  />
                ))}
                {hasActions && <ActionsTh />}
              </tr>
            </thead>
            <tbody>
              <VirtualSpacerRow height={paddingTop} colSpan={columnCount} />
              {virtualItems.map((vi) => {
                const i = order[vi.index];
                const id = elements.ids[i];
                const isSelected = id === activeId;
                return (
                  <tr
                    key={id}
                    data-selected={isSelected ? "true" : undefined}
                    onClick={() => onSelect?.(id)}
                    {...editorRowHover}
                    style={editorRowStyle({
                      selected: isSelected,
                      clickable: !!onSelect,
                    })}
                  >
                    <td style={{ ...EDITOR_TD, fontWeight: 500 }}>{id}</td>
                    {placed &&
                      (["x", "y"] as const).map((axis, ai) => {
                        const at = elements.positions[i];
                        // An element the model places nowhere shows a
                        // dash rather than a zero: nowhere is not the
                        // origin, and a table that said 0 would invite
                        // someone to believe it.
                        if (!at || !onMove) {
                          return (
                            <td
                              key={axis}
                              style={{ ...EDITOR_TD, textAlign: "right" }}
                            >
                              {at ? at[ai] : "—"}
                            </td>
                          );
                        }
                        return (
                          <td key={axis} style={{ ...EDITOR_TD, padding: 0 }}>
                            <EditableNumber
                              value={at[ai]}
                              sys={sys}
                              label={`${id} ${axis.toUpperCase()}`}
                              chrome="cell"
                              align="right"
                              onCommit={(next) =>
                                onMove(
                                  id,
                                  ai === 0 ? next : at[0],
                                  ai === 1 ? next : at[1],
                                )
                              }
                            />
                          </td>
                        );
                      })}
                    {elements.columns.map((c, ci) => {
                      const v = c.values[i];
                      const align = numeric[ci] ? "right" : "left";
                      // What this cell offers is the column's declared
                      // shape, not a guess from the value: a valve type
                      // is a choice of seven and a check valve is a
                      // yes/no, and neither is a box to type in.
                      const editor = cellEditor(c, v, !!onEdit);
                      if (editor.kind === "number") {
                        return (
                          <td key={c.key} style={{ ...EDITOR_TD, padding: 0 }}>
                            <EditableNumber
                              value={editor.value}
                              quantity={c.quantity}
                              sys={sys}
                              // The column heading already carries the
                              // unit, and the row is identified by its
                              // id — together that is what a screen
                              // reader needs to place the field.
                              label={`${id} ${c.label}`}
                              chrome="cell"
                              align={align}
                              onCommit={(next) =>
                                onEdit?.(id, c.key, next, editor.value)
                              }
                            />
                          </td>
                        );
                      }
                      if (editor.kind === "choice") {
                        return (
                          <td key={c.key} style={{ ...EDITOR_TD, padding: 0 }}>
                            <CellSelect
                              label={`${id} ${c.label}`}
                              value={editor.value}
                              items={editor.items}
                              onCommit={(next) =>
                                onEdit?.(id, c.key, next, editor.value)
                              }
                            />
                          </td>
                        );
                      }
                      if (editor.kind === "text") {
                        return (
                          <td key={c.key} style={{ ...EDITOR_TD, padding: 0 }}>
                            <CellText
                              label={`${id} ${c.label}`}
                              value={editor.value}
                              onCommit={(next) =>
                                onEdit?.(id, c.key, next, editor.value)
                              }
                            />
                          </td>
                        );
                      }
                      return (
                        <td
                          key={c.key}
                          style={{ ...EDITOR_TD, textAlign: align }}
                        >
                          {v == null
                            ? "—"
                            : typeof v === "number"
                              ? formatElementAttribute(
                                  {
                                    // Formatting only — this row is a
                                    // table cell, not an addressable
                                    // attribute.
                                    key: "",
                                    editable: false,
                                    label: c.label,
                                    number: v,
                                    quantity: c.quantity,
                                  },
                                  sys,
                                )
                              : v}
                        </td>
                      );
                    })}
                    {hasActions && (
                      <RowActionsCell selected={isSelected}>
                        {onReveal && (
                          <ActionIcon
                            title="Show on map"
                            onClick={() => onReveal(id)}
                          >
                            <MapPinIcon style={{ width: 13, height: 13 }} />
                          </ActionIcon>
                        )}
                        {onRename && (
                          <ActionIcon
                            title="Rename"
                            onClick={() => onRename(id)}
                          >
                            <PencilSquareIcon
                              style={{ width: 13, height: 13 }}
                            />
                          </ActionIcon>
                        )}
                        {onDelete && (
                          <ActionIcon
                            title="Delete"
                            danger
                            onClick={() => onDelete(id)}
                          >
                            <TrashIcon style={{ width: 13, height: 13 }} />
                          </ActionIcon>
                        )}
                      </RowActionsCell>
                    )}
                  </tr>
                );
              })}
              <VirtualSpacerRow height={paddingBottom} colSpan={columnCount} />
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

/** Shared chrome for the two cell editors that are not numbers: no
 * border at rest, a focus ring while in use, the cell's own padding. */
const CELL_INPUT: React.CSSProperties = {
  display: "block",
  width: "100%",
  boxSizing: "border-box",
  padding: "7px 10px",
  background: "transparent",
  border: "none",
  outline: "none",
  borderRadius: 0,
  color: "var(--text-primary)",
  fontFamily: "var(--font-mono)",
  fontSize: "var(--text-md)",
};

/** A cell whose value is one of a declared list. */
function CellSelect({
  label,
  value,
  items,
  onCommit,
}: {
  label: string;
  value: string;
  items: Array<{ value: string; label: string }>;
  onCommit: (value: string) => void;
}) {
  return (
    <select
      aria-label={label}
      value={value}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => {
        if (e.target.value !== value) onCommit(e.target.value);
      }}
      style={{ ...CELL_INPUT, cursor: "pointer" }}
    >
      {/* A value the engine holds that the list does not offer still has
          to be shown, or the select would silently claim the element is
          something it is not. */}
      {!items.some((i) => i.value === value) && (
        <option value={value}>{value}</option>
      )}
      {items.map((i) => (
        <option key={i.value} value={i.value}>
          {i.label}
        </option>
      ))}
    </select>
  );
}

/** A cell whose value is free text — a reference to another element,
 * most often. Committed on blur or Enter, abandoned on Escape, and
 * silent when unchanged, exactly as the numeric field is. */
function CellText({
  label,
  value,
  onCommit,
}: {
  label: string;
  value: string;
  onCommit: (value: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <input
      aria-label={label}
      value={draft}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => {
        if (draft !== value) onCommit(draft);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") {
          setDraft(value);
          e.currentTarget.blur();
        }
      }}
      style={CELL_INPUT}
    />
  );
}
