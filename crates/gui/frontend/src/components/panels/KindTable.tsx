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
import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { KindElements } from "../../hooks";
import { formatElementAttribute } from "../../hooks/network";
import { readTextScale } from "../../textScale";
import { useUnitSystem } from "../../units";
import { AttributeField } from "./attributeField";
import { cellEditor } from "./cellEditor";
import {
  ActionIcon,
  ActionsTh,
  EDITOR_TD,
  editorRowHeight,
  editorRowHover,
  editorRowStyle,
  offerDatalist,
  RowActionsCell,
  SortTh,
  useVirtualRows,
  VirtualSpacerRow,
} from "./editorTable";

type SortDir = "asc" | "desc";

/** The two end columns, in the order that is the sign convention for
 *  whatever the line carries (hydra-common §4.5.2.1) — never sorted or
 *  swapped, because reversing them reverses the element. */
const END_COLUMNS = [
  { field: "from", label: "From" },
  { field: "to", label: "To" },
] as const;

/** Datalist key for the ends, which share one list. Leading space so no
 *  attribute key can collide with it. */
const END_LIST = " ends";

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
  onReconnect,
  endIds,
  referenceIds,
  onAdd,
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
   * Point one line at two other elements, first end then second.
   *
   * Separate from `onEdit` for the same reason `onMove` is (hydra-common
   * §4.5.2.1): an end is implied by the `polyline` class, has no schema
   * key to be addressed by, and is written by its own operation. Both
   * ends go together even to change one, so the cell that changed sends
   * the value beside it unchanged.
   *
   * Absent, or a kind that is not a line, and the columns are read-only.
   */
  onReconnect?: (
    id: string,
    fromId: string,
    toId: string,
  ) => Promise<void> | void;
  /**
   * Ids the two end columns may name.
   *
   * A separate prop from `referenceIds` because an end is not a
   * reference in the §4.5.1.1 sense: it names no single declared kind,
   * since a line in a real model may run to several kinds of thing. The
   * caller decides which elements can be an end; this table offers what
   * it is given.
   */
  endIds?: string[];
  /**
   * Per-row actions, each offered only when its handler is given.
   *
   * A table of elements is where a reader finds one, so the things they
   * then want to do to it belong on the row rather than behind a
   * selection and a trip elsewhere. An engine that cannot rename shows
   * no rename; the column disappears entirely when none are given.
   */
  /**
   * Ids this table's reference columns may name, by kind — the answer
   * to a column's `references`.
   *
   * Supplied rather than fetched, so this stays a component that draws
   * what it is given. Absent for a kind, and its cells are plain text
   * fields: a reference with no list is still typeable, and the engine
   * still refuses a name that means nothing.
   */
  referenceIds?: Record<string, string[]>;
  /**
   * Add an element of this kind. Absent, and no button appears — which
   * is what a kind that cannot be created gets, and a table with
   * nowhere to put a new one.
   */
  onAdd?: () => void;
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
    const end = sortCol === "from" ? 0 : sortCol === "to" ? 1 : null;
    const get = (i: number): number | string | null => {
      if (propCol) return propCol.values[i] ?? null;
      if (axis != null) return elements.positions[i]?.[axis] ?? null;
      if (end != null) return elements.ends[i]?.[end] ?? null;
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
  // One datalist per referenced kind, not per cell: the options are the
  // same down a column, and a copy per row is tens of thousands of
  // `<option>` nodes rebuilt on every scroll — which hangs the tab
  // outright at model scale.
  const listPrefix = useId();
  const lists = useMemo(() => {
    const out: Array<{ key: string; ids: string[] }> = [];
    for (const c of elements.columns) {
      // The union across every kind the column may name, sorted so the
      // list reads as one set of ids rather than as several lists run
      // together — a reader scanning for "J12" should not have to know
      // which kind it is to know where to look.
      const named = [
        ...new Set(
          (c.references ?? []).flatMap((k) => referenceIds?.[k] ?? []),
        ),
      ].sort();
      // Undefined rather than empty, so a column whose kinds the caller
      // supplied nothing for stays a plain field — an empty list is a
      // list, and the browser draws it as one that offers nothing.
      const ids = named.length ? named : undefined;
      // Above the cutoff the list is dropped rather than truncated: a
      // shortened list silently hides valid ids, and the browser's own
      // filter is the bottleneck at that size anyway. The cell stays a
      // text field and the engine still judges what was typed.
      if (ids && offerDatalist(ids.length)) out.push({ key: c.key, ids });
    }
    // The ends share one list between them, under a key no schema can
    // collide with — an attribute key is an identifier, so it cannot
    // start with a space.
    if (endIds && offerDatalist(endIds.length)) {
      out.push({ key: END_LIST, ids: endIds });
    }
    return out;
  }, [elements.columns, referenceIds, endIds]);

  // An empty kind has no positions and no ids, and 0 === 0 would put X
  // and Y on a table of curves. Unreachable today — an empty kind
  // returns before the table is drawn — but the derivation should be
  // true on its own terms.
  const placed =
    elements.ids.length > 0 &&
    elements.positions.length === elements.ids.length;
  // A line is not at a place, it runs between two — so a kind has one of
  // these or the other, never both, and each is derived from the arrays
  // actually being parallel rather than from the kind's name.
  const joined =
    elements.ids.length > 0 && elements.ends.length === elements.ids.length;
  const hasActions = !!(onReveal || onRename || onDelete);
  const columnCount =
    elements.columns.length +
    1 +
    (placed ? 2 : 0) +
    (joined ? 2 : 0) +
    (hasActions ? 1 : 0);

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
      {/* The bar above every editor table: 44px tall, search right-
          aligned, Add on the left when the catalog says the kind can be
          created. Both engines get the same one — which is the point of
          the shared table. */}
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
        {onAdd && (
          <button
            type="button"
            onClick={onAdd}
            style={{
              marginLeft: 8,
              height: 28,
              padding: "0 10px",
              background: "var(--accent-dim)",
              color: "var(--accent)",
              border: "1px solid var(--border-focus)",
              borderRadius: 5,
              fontSize: "var(--text-md)",
              fontWeight: 500,
              cursor: "pointer",
            }}
          >
            + Add
          </button>
        )}
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
                {joined &&
                  END_COLUMNS.map(({ field, label }) => (
                    <SortTh
                      key={field}
                      field={field}
                      label={label}
                      sortField={sortCol}
                      sortAsc={sortDir === "asc"}
                      onSort={toggleSort}
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
                            <AttributeField
                              editor={{ kind: "number", value: at[ai] }}
                              sys={sys}
                              label={`${id} ${axis.toUpperCase()}`}
                              chrome="cell"
                              align="right"
                              onCommit={(next) =>
                                onMove(
                                  id,
                                  ai === 0 ? Number(next) : at[0],
                                  ai === 1 ? Number(next) : at[1],
                                )
                              }
                            />
                          </td>
                        );
                      })}
                    {joined &&
                      END_COLUMNS.map(({ field, label }, ei) => {
                        const ends = elements.ends[i];
                        if (!ends) {
                          return (
                            <td key={field} style={EDITOR_TD}>
                              —
                            </td>
                          );
                        }
                        if (!onReconnect) {
                          return (
                            <td key={field} style={EDITOR_TD}>
                              {ends[ei]}
                            </td>
                          );
                        }
                        return (
                          <td key={field} style={{ ...EDITOR_TD, padding: 0 }}>
                            <AttributeField
                              editor={{ kind: "text", value: ends[ei] }}
                              label={`${id} ${label}`}
                              sys={sys}
                              chrome="cell"
                              listId={
                                lists.some((l) => l.key === END_LIST)
                                  ? `${listPrefix}-${END_LIST}`
                                  : undefined
                              }
                              onCommit={(next) =>
                                onReconnect(
                                  id,
                                  ei === 0 ? String(next) : ends[0],
                                  ei === 1 ? String(next) : ends[1],
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
                      if (editor.kind !== "none") {
                        return (
                          <td key={c.key} style={{ ...EDITOR_TD, padding: 0 }}>
                            <AttributeField
                              editor={editor}
                              quantity={c.quantity}
                              sys={sys}
                              // The column heading already carries the
                              // unit, and the row is identified by its
                              // id — together that is what a screen
                              // reader needs to place the field.
                              label={`${id} ${c.label}`}
                              chrome="cell"
                              align={align}
                              listId={
                                lists.some((l) => l.key === c.key)
                                  ? `${listPrefix}-${c.key}`
                                  : undefined
                              }
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
                                    kind: c.kind,
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
