import { ChevronDownIcon, ChevronUpIcon } from "@heroicons/react/16/solid";
import type React from "react";
import { useRef, useState } from "react";

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
}) {
  const editValue = value ?? display;
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

  function validate(raw: string): string | null {
    if (inputType !== "number") return null;
    const n = parseFloat(raw);
    if (!Number.isFinite(n)) return "Must be a number";
    if (min !== undefined && n < min) return `Min ${min}`;
    if (max !== undefined && n > max) return `Max ${max}`;
    return null;
  }

  function handleFocus() {
    focusSnapshot.current = draft;
    setFocused(true);
    setError(null);
  }

  function commit() {
    setFocused(false);
    const trimmed = draft.trim();
    const err = validate(trimmed);
    if (err) {
      // Invalid: show error and revert to last committed value.
      setError(err);
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
        fontSize: 12,
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
      <input
        value={focused || isPending ? draft : display}
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
          color: isError
            ? "rgba(220,80,80,0.9)"
            : !focused && placeholder
              ? "var(--text-tertiary)"
              : "var(--text-primary)",
          fontFamily: "var(--font-mono)",
          fontSize: 12,
          textAlign: align ?? "left",
        }}
      />
    </td>
  );
}

/* ── Reference input cell ───────────────────────────────────────────────────── */

/**
 * A searchable reference input backed by a datalist.
 * Useful for fields that must point at an existing element ID.
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
  listId: string;
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
        fontSize: 12,
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
          fontSize: 12,
          textAlign: align ?? "left",
        }}
      />
      <datalist id={listId}>
        {options.map((opt) => (
          <option key={opt} value={opt} />
        ))}
      </datalist>
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
      onClick={() => onSort(field)}
      style={{
        fontSize: 11,
        fontWeight: 500,
        color: isActive ? "var(--text-secondary)" : "var(--text-tertiary)",
        textAlign: align ?? "left",
        padding: "8px 10px",
        borderBottom: "1px solid var(--border)",
        whiteSpace: "nowrap",
        cursor: "pointer",
        userSelect: "none",
        position: "sticky",
        top: 0,
        background: "var(--bg-panel)",
        zIndex: 1,
        ...style,
      }}
    >
      {label}
      {isActive && (
        <span
          style={{
            marginLeft: 4,
            fontSize: 10,
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
    </th>
  );
}
