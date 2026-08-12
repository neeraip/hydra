// ── What every element table in the app is made of ────────────────────────────
//
// The row metrics, the sortable header, and the virtualisation. None of it
// is engine knowledge or even editing knowledge — a table of elements
// looks and scrolls the same whether its rows can be changed or only read.
//
// It lived under `pages/project/NetworkEditor/` because the
// water-distribution editor was written first, and two surfaces outside
// that folder were already importing it from there. Meanwhile the
// drainage editor grew its own table and drifted: uppercase headers, rows
// four pixels shorter, fainter separators, no hover, a sort mark on every
// column, and every row mounted at once. One home, so the next table has
// somewhere to come from.
//
// What stays behind in `TablePrimitives` is the water-distribution
// editor's *cells* — they carry staged-draft machinery (pending markers,
// discard generations) that only an editor with a Save button has.

import {
  ChevronDownIcon,
  ChevronUpDownIcon,
  ChevronUpIcon,
} from "@heroicons/react/16/solid";
import { useVirtualizer } from "@tanstack/react-virtual";
import type React from "react";
import { readTextScale } from "../../textScale";

/* ── Row metrics ─────────────────────────────────────────────────────────── */

/** Top + bottom of a cell's `padding: "7px 10px"`. Literal pixels, so this
 * part of the row height does not move with the text scale. */
const CELL_PADDING_Y = 14;
/** Row height at text scale 1: the padding above plus the ~16px line box of
 * 13px cell text. */
const ROW_HEIGHT_AT_SCALE_1 = 30;

/**
 * Height of one editor row at the current text scale.
 *
 * These tables are the only virtualised list in the app that estimates row
 * height rather than measuring it, and callers also multiply by it to scroll
 * to a row that isn't mounted yet. So the value has to track the text scale:
 * the error repeats once per row, and these tables run to tens of thousands
 * of rows — a few pixels off becomes a scrollbar that doesn't reach the end
 * and a "reveal element" that centres the wrong row.
 *
 * Only the cell's text scales; its padding is fixed. So the row grows by less
 * than the scale factor, and interpolating just the text portion is exact
 * rather than merely closer than a constant.
 */
export function editorRowHeight(scale: number): number {
  return Math.round(
    CELL_PADDING_Y + (ROW_HEIGHT_AT_SCALE_1 - CELL_PADDING_Y) * scale,
  );
}

/**
 * A read-only body cell.
 *
 * The padding is what `editorRowHeight` is computed from, so a table that
 * sets its own would scroll wrong once virtualised — which is the concrete
 * reason this is shared rather than copied. An editable cell sets
 * `padding: 0` and puts this padding on the input inside it, so the two
 * kinds of cell are the same height.
 */
export const EDITOR_TD: React.CSSProperties = {
  padding: "7px 10px",
  fontSize: "var(--text-md)",
  fontFamily: "var(--font-mono)",
  borderBottom: "1px solid var(--border)",
};

/**
 * A body row's own styling: selected, or staged, or neither.
 *
 * Selection is a filled row with an accent bar down its left edge, not an
 * outline — an outline reads as a focus ring, and the row is a place you
 * are rather than a control you have focused.
 */
export function editorRowStyle({
  selected,
  pending,
  clickable,
}: {
  selected: boolean;
  /** Has unsaved staged edits. Editors without a Save button never set it. */
  pending?: boolean;
  clickable?: boolean;
}): React.CSSProperties {
  return {
    cursor: clickable ? "pointer" : undefined,
    background: selected
      ? "var(--accent-dim)"
      : pending
        ? "rgba(220, 160, 40, 0.05)"
        : undefined,
    borderLeft: selected ? "2px solid var(--accent)" : "2px solid transparent",
  };
}

/** Lift a row on hover, unless it is already the selected one. */
export const editorRowHover = {
  onMouseEnter: (e: React.MouseEvent<HTMLTableRowElement>) => {
    if (e.currentTarget.dataset.selected !== "true") {
      e.currentTarget.style.background = "var(--bg-card-hover)";
    }
  },
  onMouseLeave: (e: React.MouseEvent<HTMLTableRowElement>) => {
    if (e.currentTarget.dataset.selected !== "true") {
      e.currentTarget.style.background = "";
    }
  },
};

/* ── Virtualisation ──────────────────────────────────────────────────────── */

/**
 * Mount only the rows actually inside the scroll container, so a network
 * with thousands of elements does not render every row up front.
 *
 * `scrollRef` must point at the actual scrolling ancestor, so `<thead>`'s
 * `position: sticky` headers keep working unmodified.
 */
export function useVirtualRows<T>(
  rows: T[],
  scrollRef: React.RefObject<HTMLDivElement | null>,
) {
  // Read once per render rather than inside `estimateSize`, which the
  // virtualizer calls per row — on a 46k-row table that would be 46k reads.
  const rowHeight = editorRowHeight(readTextScale());
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 12,
  });
  const virtualItems = virtualizer.getVirtualItems();
  const paddingTop = virtualItems.length > 0 ? virtualItems[0].start : 0;
  const paddingBottom =
    virtualItems.length > 0
      ? virtualizer.getTotalSize() - virtualItems[virtualItems.length - 1].end
      : 0;
  return { virtualItems, paddingTop, paddingBottom, virtualizer };
}

/** Spacer `<tr>` used above/below the rendered window to preserve the
 * scrollbar's total height without mounting every row. */
export function VirtualSpacerRow({
  height,
  colSpan,
}: {
  height: number;
  colSpan: number;
}) {
  if (height <= 0) return null;
  return (
    <tr aria-hidden style={{ height }}>
      <td colSpan={colSpan} style={{ padding: 0, border: "none" }} />
    </tr>
  );
}

/* ── Sort header cell ────────────────────────────────────────────────────── */

export function SortTh({
  field,
  label,
  sortField,
  sortAsc,
  onSort,
  align,
  style,
  /**
   * Draw a dimmed mark on the unsorted columns as well.
   *
   * For a table whose columns are engine-authored and therefore
   * unfamiliar: the mark says "this one sorts too", which a reader of a
   * junction table already knows and a reader of a table of inlet
   * designs does not.
   */
  markUnsorted,
}: {
  field: string;
  label: string;
  sortField: string | null;
  sortAsc: boolean;
  onSort: (f: string) => void;
  align?: "left" | "right";
  style?: React.CSSProperties;
  markUnsorted?: boolean;
}) {
  const isActive = sortField === field;
  return (
    <th
      aria-sort={isActive ? (sortAsc ? "ascending" : "descending") : "none"}
      style={{
        fontSize: "var(--text-sm)",
        fontWeight: 500,
        color: isActive ? "var(--text-secondary)" : "var(--text-tertiary)",
        textAlign: align ?? "left",
        padding: "8px 10px",
        borderBottom: "1px solid var(--border)",
        whiteSpace: "nowrap",
        userSelect: "none",
        position: "sticky",
        top: 0,
        background: "var(--bg-panel)",
        zIndex: 1,
        ...style,
      }}
    >
      {/* Real <button> so the header is keyboard-focusable and Enter/Space
          toggle sorting natively; .th-sort-btn inherits every font style so
          the rendered layout is identical to a bare label. */}
      <button
        type="button"
        className="th-sort-btn"
        onClick={() => onSort(field)}
        style={{
          justifyContent: align === "right" ? "flex-end" : "flex-start",
        }}
      >
        {label}
        {isActive ? (
          <span
            style={{
              fontSize: "var(--text-xs)",
              display: "inline-flex",
              alignItems: "center",
            }}
          >
            {sortAsc ? (
              <ChevronUpIcon style={{ width: 12, height: 12 }} />
            ) : (
              <ChevronDownIcon style={{ width: 12, height: 12 }} />
            )}
          </span>
        ) : (
          markUnsorted && (
            <span
              style={{
                fontSize: "var(--text-xs)",
                display: "inline-flex",
                alignItems: "center",
                opacity: 0.25,
              }}
            >
              <ChevronUpDownIcon style={{ width: 12, height: 12 }} />
            </span>
          )
        )}
      </button>
    </th>
  );
}

/* ── Row actions ─────────────────────────────────────────────────────────── */

/** Trailing header cell for the actions column (blank, narrow). */
export function ActionsTh() {
  return (
    <th
      aria-label="Actions"
      style={{
        width: 1,
        borderBottom: "1px solid var(--border)",
        padding: "7px 10px",
      }}
    />
  );
}

const ACTION_BUTTON: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  width: 22,
  height: 22,
  padding: 0,
  border: "none",
  borderRadius: 4,
  background: "transparent",
  cursor: "pointer",
};

/**
 * One icon action on a row.
 *
 * `disabledReason` doubles as the tooltip: an action that is off has to
 * say why, or the reader is left to guess whether it is broken. An
 * action with no reason and no handler is simply not rendered by its
 * caller — a permanently dead icon teaches nothing.
 */
export function ActionIcon({
  title,
  disabledReason,
  danger,
  onClick,
  children,
}: {
  title: string;
  disabledReason?: string;
  danger?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  const disabled = disabledReason != null;
  const resting = disabled
    ? "var(--text-disabled)"
    : danger
      ? "rgba(230, 120, 120, 0.9)"
      : "var(--text-secondary)";
  return (
    <button
      type="button"
      disabled={disabled}
      data-tooltip={disabledReason ?? title}
      aria-label={title}
      onClick={(e) => {
        // The row beneath is selectable; an action is not a selection.
        e.stopPropagation();
        if (!disabled) onClick();
      }}
      style={{
        ...ACTION_BUTTON,
        color: resting,
        cursor: disabled ? "not-allowed" : "pointer",
      }}
      onMouseEnter={(e) => {
        if (disabled) return;
        e.currentTarget.style.background = "var(--bg-card-hover)";
        e.currentTarget.style.color = danger
          ? "rgb(240, 130, 130)"
          : "var(--text-primary)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "transparent";
        e.currentTarget.style.color = resting;
      }}
    >
      {children}
    </button>
  );
}

/** The cell the actions sit in: narrow, right-aligned, revealed on hover
 * and kept visible on the selected row (`.ne-row-actions`). */
export function RowActionsCell({
  selected,
  children,
}: {
  selected: boolean;
  children: React.ReactNode;
}) {
  return (
    <td
      style={{
        borderBottom: "1px solid var(--border)",
        padding: "0 8px",
        textAlign: "right",
        whiteSpace: "nowrap",
        width: 1,
      }}
    >
      <div
        className={`ne-row-actions${selected ? " is-visible" : ""}`}
        style={{ display: "inline-flex", gap: 1 }}
      >
        {children}
      </div>
    </td>
  );
}

/* ── Reference completions ───────────────────────────────────────────────── */

/**
 * Above this many options a reference `<datalist>` is dropped entirely.
 *
 * One shared list is fine at moderate sizes — it is N nodes rendered
 * once per table — but at tens of thousands the browser's own typing
 * filter becomes the bottleneck, re-scanning the whole list on the UI
 * thread at every keystroke.
 */
export const REF_DATALIST_MAX_OPTIONS = 5000;

/**
 * Whether to offer completions for this many ids.
 *
 * Dropped rather than truncated above the cutoff: a shortened list
 * silently hides valid ids while still looking authoritative, and the
 * cell remains a text field either way — the engine judges what was
 * typed, so losing the list costs convenience and never correctness.
 */
export function offerDatalist(optionCount: number): boolean {
  return optionCount <= REF_DATALIST_MAX_OPTIONS;
}
