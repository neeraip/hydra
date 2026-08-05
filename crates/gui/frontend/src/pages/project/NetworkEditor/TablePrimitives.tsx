import { ChevronDownIcon, ChevronUpIcon } from "@heroicons/react/16/solid";
import { useVirtualizer } from "@tanstack/react-virtual";
import type React from "react";
import { memo, useId, useRef, useState } from "react";
import { readTextScale } from "../../../textScale";
import { parseNumericInput } from "../../../units";
import { referenceError } from "./referenceIds";
import { shouldUseRefDatalist } from "./tableSearch";

/* ── Row virtualization ──────────────────────────────────────────────────────── */

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
 * height rather than measuring it, and `ElementsEditor` also multiplies by it
 * to scroll to a row that isn't mounted yet. So the value has to track the
 * text scale: the error repeats once per row, and these tables run to tens of
 * thousands of rows — a few pixels off becomes a scrollbar that doesn't reach
 * the end and a "reveal element" that centres the wrong row.
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
 * Shared virtualization for the Editor's element tables, so only the rows
 * actually visible in the scroll container are mounted — large networks
 * (thousands of junctions/pipes) don't render every row up front.
 *
 * `scrollRef` must point at the actual scrolling ancestor (owned by
 * ElementsEditor, shared across whichever table is active) so `<thead>`'s
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

/* ── EditableCell ────────────────────────────────────────────────────────────── */

/**
 * A `<td>` that renders an always-visible `<input>` styled to look like a
 * plain table cell.  The input has no visible border or background at rest;
 * it gains a subtle focus ring when the user clicks into it, without changing
 * the size of the cell or the layout of the surrounding table.
 *
 * - **Enter** or **blur** → commits the change via `onCommit`
 * - **Escape**            → reverts to the last committed value
 */
export function EditableCell({
  display,
  value,
  placeholder,
  align,
  style,
  onCommit,
  isPending,
  inputType = "text",
  min,
  max,
  options,
}: {
  /** Text shown in read mode (the cell label). */
  display: string;
  /** Value pre-filled into the input when editing begins. Defaults to `display`.
   *  Useful for nullable fields where `display` is a placeholder like "—". */
  value?: string;
  /** When true, `display` is treated as a placeholder and rendered dimly. */
  placeholder?: boolean;
  align?: "left" | "right";
  style?: React.CSSProperties;
  onCommit: (value: string) => void;
  /** When true, renders an amber left-border to mark an unsaved draft change. */
  isPending?: boolean;
  /** "text" (default) or "number" — determines validation behaviour. */
  inputType?: "text" | "number";
  /** Inclusive minimum for number inputs. */
  min?: number;
  /** Inclusive maximum for number inputs. */
  max?: number;
  /**
   * For cells holding a *reference* to another element: the ids that may
   * be named. Offered as suggestions while typing, and enforced on commit
   * — a reference to something that does not exist is a dangling one, and
   * the cell that accepted it is where it should be refused.
   */
  options?: readonly string[];
}) {
  const editValue = value ?? display;
  const datalistId = useId();
  const [draft, setDraft] = useState(editValue);
  const [focused, setFocused] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // Snapshot of the draft value at the moment the input was focused.
  // commit() compares against this to avoid marking clean blurs as dirty.
  const focusSnapshot = useRef(editValue);

  // Keep draft in sync when the committed value changes from outside (e.g.
  // after a save round-trip reloads fresh data).
  const prevEditValue = useRef(editValue);
  if (editValue !== prevEditValue.current) {
    prevEditValue.current = editValue;
    setDraft(editValue);
  }

  // Strict parse via parseNumericInput: rejects interleaved garbage that
  // parseFloat would prefix-salvage ("8F.6G2Y" → 8), while tolerating a
  // pasted display unit ("8.62 m" → 8.62).
  function validate(raw: string): { err: string | null; normalized: string } {
    if (options) {
      return { err: referenceError(raw, options), normalized: raw };
    }
    if (inputType !== "number") return { err: null, normalized: raw };
    const parsed = parseNumericInput(raw);
    if (parsed.kind !== "number")
      return { err: "Must be a number", normalized: raw };
    if (min !== undefined && parsed.value < min)
      return { err: `Min ${min}`, normalized: raw };
    if (max !== undefined && parsed.value > max)
      return { err: `Max ${max}`, normalized: raw };
    return { err: null, normalized: String(parsed.value) };
  }

  function handleFocus() {
    focusSnapshot.current = draft;
    setFocused(true);
    setError(null);
  }

  function commit() {
    setFocused(false);
    const trimmed = draft.trim();
    // Cleared number cell = abandoned edit: revert silently, commit nothing.
    if (inputType === "number" && trimmed === "") {
      setError(null);
      setDraft(focusSnapshot.current);
      return;
    }
    const { err, normalized } = validate(trimmed);
    if (err) {
      // Invalid: show error and revert to last committed value.
      setError(err);
      setDraft(focusSnapshot.current);
      return;
    }
    setError(null);
    // Commit the normalized value (unit suffix stripped) so downstream
    // parseFloat consumers always receive a clean number string.
    if (normalized !== focusSnapshot.current.trim()) onCommit(normalized);
  }

  // Block letter keystrokes in number cells for immediate feedback; paste is
  // deliberately allowed through so unit-suffixed values ("8.62 m") can be
  // normalised at commit time.
  function onBeforeInput(e: React.FormEvent<HTMLInputElement>) {
    if (inputType !== "number") return;
    const native = e.nativeEvent as InputEvent;
    if (native.inputType !== "insertText" || native.data == null) return;
    if (/[^0-9.eE+-]/.test(native.data)) e.preventDefault();
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      e.preventDefault();
      (e.target as HTMLInputElement).blur();
    }
    if (e.key === "Escape") {
      setDraft(focusSnapshot.current);
      setError(null);
      (e.target as HTMLInputElement).blur();
    }
  }

  const isError = !!error;

  return (
    <td
      style={{
        padding: 0,
        fontSize: "var(--text-md)",
        fontFamily: "var(--font-mono)",
        borderBottom: "1px solid var(--border)",
        textAlign: align ?? "left",
        borderLeft: isError
          ? "2px solid rgba(220,60,60,0.7)"
          : isPending
            ? "2px solid rgba(220, 160, 40, 0.65)"
            : undefined,
        position: "relative",
        ...style,
      }}
      title={error ?? undefined}
    >
      {/* Suggestions, not a closed select: the ids are the model's own and
          a list is how you discover them, while typing still reaches a
          long one faster than scrolling does. Validity is enforced on
          commit rather than by the widget. */}
      {options && (
        <datalist id={datalistId}>
          {options.map((o) => (
            <option key={o} value={o} />
          ))}
        </datalist>
      )}
      <input
        list={options ? datalistId : undefined}
        value={focused || isPending ? draft : display}
        onChange={(e) => {
          setDraft(e.target.value);
          setError(null);
        }}
        onFocus={handleFocus}
        onBlur={commit}
        onKeyDown={onKeyDown}
        onBeforeInput={onBeforeInput}
        onClick={(e) => e.stopPropagation()}
        style={{
          display: "block",
          width: "100%",
          boxSizing: "border-box",
          padding: "7px 10px",
          background: isError
            ? "rgba(220,60,60,0.08)"
            : focused
              ? "var(--bg-input, rgba(255,255,255,0.05))"
              : isPending
                ? "rgba(220, 160, 40, 0.05)"
                : "transparent",
          border: "none",
          outline: isError
            ? "1px solid rgba(220,60,60,0.5)"
            : focused
              ? "1px solid var(--border-focus, rgba(100,160,255,0.5))"
              : "none",
          outlineOffset: "-1px",
          borderRadius: 0,
          color: isError
            ? "rgba(220,80,80,0.9)"
            : !focused && placeholder
              ? "var(--text-tertiary)"
              : "var(--text-primary)",
          fontFamily: "var(--font-mono)",
          fontSize: "var(--text-md)",
          textAlign: align ?? "left",
        }}
      />
    </td>
  );
}

/* ── SelectCell ─────────────────────────────────────────────────────────────── */

/**
 * A `<td>` wrapping a compact `<select>` styled like {@link EditableCell}:
 * no visible chrome at rest, a subtle focus ring while open, and the same
 * amber pending marker for staged (unsaved) changes.
 *
 * Mirrors EditableCell's draft handling: the picked value is held locally so
 * the cell keeps showing it while the change is only staged, and re-syncs
 * when the committed value changes from outside (e.g. after a save
 * round-trip). Remount via a `discardGen`-keyed `key` resets the draft on
 * discard, exactly like the input cells.
 */
export function SelectCell({
  value,
  options,
  onCommit,
  isPending,
  align,
  style,
}: {
  /** Committed value (must match one option's `value`). */
  value: string;
  options: ReadonlyArray<{ value: string; label: string }>;
  onCommit: (value: string) => void;
  /** When true, renders an amber left-border to mark an unsaved draft change. */
  isPending?: boolean;
  align?: "left" | "right";
  style?: React.CSSProperties;
}) {
  const [draft, setDraft] = useState(value);
  const [focused, setFocused] = useState(false);

  // Re-sync when the committed value changes from outside.
  const prevValue = useRef(value);
  if (value !== prevValue.current) {
    prevValue.current = value;
    setDraft(value);
  }

  return (
    <td
      style={{
        padding: 0,
        fontSize: "var(--text-md)",
        fontFamily: "var(--font-mono)",
        borderBottom: "1px solid var(--border)",
        textAlign: align ?? "left",
        borderLeft: isPending
          ? "2px solid rgba(220, 160, 40, 0.65)"
          : undefined,
        position: "relative",
        ...style,
      }}
    >
      <select
        value={isPending || focused ? draft : value}
        onChange={(e) => {
          const next = e.target.value;
          setDraft(next);
          if (next !== value) onCommit(next);
        }}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onClick={(e) => e.stopPropagation()}
        style={{
          display: "block",
          width: "100%",
          boxSizing: "border-box",
          padding: "6px 7px",
          background: focused
            ? "var(--bg-input, rgba(255,255,255,0.05))"
            : isPending
              ? "rgba(220, 160, 40, 0.05)"
              : "transparent",
          border: "none",
          outline: focused
            ? "1px solid var(--border-focus, rgba(100,160,255,0.5))"
            : "none",
          outlineOffset: "-1px",
          borderRadius: 0,
          color: "var(--text-primary)",
          fontFamily: "var(--font-mono)",
          fontSize: "var(--text-md)",
          textAlign: align ?? "left",
          cursor: "pointer",
        }}
      >
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </td>
  );
}

/* ── Reference input cell ───────────────────────────────────────────────────── */

/**
 * The single `<datalist>` shared by every {@link RefInputCell} of a table.
 *
 * Each RefInputCell used to render its own copy of the full option list with
 * a unique per-row list id — at ~46k node ids that meant tens of thousands of
 * `<option>` elements per cell, recreated on scroll and on every keystroke,
 * which hangs the tab outright. Options are identical across rows, so one
 * memoized datalist per table (stable id, referenced by every input) renders
 * them at most once.
 *
 * Decision for very large option lists: above `REF_DATALIST_MAX_OPTIONS`
 * (5000) we render no datalist at all rather than capping or lazy-filling
 * it — a truncated list silently hides valid ids while the browser's native
 * filter still lags at that size. The inputs then behave as plain text
 * inputs with validation-on-blur (invalid id ⇒ existing error style), which
 * RefInputCell performs regardless of autocomplete.
 */
export const RefOptionsDatalist = memo(function RefOptionsDatalist({
  id,
  options,
}: {
  id: string;
  options: string[];
}) {
  if (!shouldUseRefDatalist(options.length)) return null;
  return (
    <datalist id={id}>
      {options.map((opt) => (
        <option key={opt} value={opt} />
      ))}
    </datalist>
  );
});

/**
 * A searchable reference input, optionally backed by a shared datalist (see
 * {@link RefOptionsDatalist}). Useful for fields that must point at an
 * existing element ID. The typed value is validated against `options` on
 * blur whether or not a datalist is attached.
 */
export function RefInputCell({
  value,
  placeholder,
  options,
  listId,
  align,
  isPending,
  onCommit,
}: {
  value: string;
  placeholder?: string;
  options: string[];
  /** Id of the table's shared datalist; omit to render a plain input. */
  listId?: string;
  align?: "left" | "right";
  isPending?: boolean;
  onCommit: (value: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  const [focused, setFocused] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const focusSnapshot = useRef(value);

  const prevValue = useRef(value);
  if (value !== prevValue.current) {
    prevValue.current = value;
    setDraft(value);
  }

  function handleFocus() {
    focusSnapshot.current = draft;
    setFocused(true);
    setError(null);
  }

  function commit() {
    setFocused(false);
    const trimmed = draft.trim();
    if (trimmed.length > 0 && !options.includes(trimmed)) {
      setError("Select a valid reference ID");
      setDraft(focusSnapshot.current);
      return;
    }
    setError(null);
    if (trimmed !== focusSnapshot.current.trim()) onCommit(trimmed);
  }

  function onKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (e.key === "Enter") {
      e.preventDefault();
      (e.target as HTMLInputElement).blur();
    }
    if (e.key === "Escape") {
      setDraft(focusSnapshot.current);
      setError(null);
      (e.target as HTMLInputElement).blur();
    }
  }

  const isError = !!error;

  return (
    <td
      style={{
        padding: 0,
        fontSize: "var(--text-md)",
        fontFamily: "var(--font-mono)",
        borderBottom: "1px solid var(--border)",
        textAlign: align ?? "left",
        borderLeft: isError
          ? "2px solid rgba(220,60,60,0.7)"
          : isPending
            ? "2px solid rgba(220, 160, 40, 0.65)"
            : undefined,
        position: "relative",
      }}
      title={error ?? undefined}
    >
      <input
        list={listId}
        value={draft}
        placeholder={placeholder}
        onChange={(e) => {
          setDraft(e.target.value);
          setError(null);
        }}
        onFocus={handleFocus}
        onBlur={commit}
        onKeyDown={onKeyDown}
        onClick={(e) => e.stopPropagation()}
        style={{
          display: "block",
          width: "100%",
          boxSizing: "border-box",
          padding: "7px 10px",
          background: isError
            ? "rgba(220,60,60,0.08)"
            : focused
              ? "var(--bg-input, rgba(255,255,255,0.05))"
              : isPending
                ? "rgba(220, 160, 40, 0.05)"
                : "transparent",
          border: "none",
          outline: isError
            ? "1px solid rgba(220,60,60,0.5)"
            : focused
              ? "1px solid var(--border-focus, rgba(100,160,255,0.5))"
              : "none",
          outlineOffset: "-1px",
          borderRadius: 0,
          color: isError ? "rgba(220,80,80,0.9)" : "var(--text-primary)",
          fontFamily: "var(--font-mono)",
          fontSize: "var(--text-md)",
          textAlign: align ?? "left",
        }}
      />
    </td>
  );
}

/* ── Sort header cell ───────────────────────────────────────────────────────── */

export function SortTh({
  field,
  label,
  sortField,
  sortAsc,
  onSort,
  align,
  style,
}: {
  field: string;
  label: string;
  sortField: string;
  sortAsc: boolean;
  onSort: (f: string) => void;
  align?: "left" | "right";
  style?: React.CSSProperties;
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
          the rendered layout is identical to the previous bare label. */}
      <button
        type="button"
        className="th-sort-btn"
        onClick={() => onSort(field)}
        style={{
          justifyContent: align === "right" ? "flex-end" : "flex-start",
        }}
      >
        {label}
        {isActive && (
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
        )}
      </button>
    </th>
  );
}
