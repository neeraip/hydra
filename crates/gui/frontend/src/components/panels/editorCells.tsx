// ── Table cells that stage an edit ───────────────────────────────────────────
//
// What is left of the water-distribution editor's own cells after that
// editor was replaced by the shared one: the two the curve and controls
// editors still use, because those editors still stage their work behind
// a Save.
//
// They are here rather than under a `NetworkEditor/` folder that no
// longer exists. The element tables' other cells — a select, a
// validating reference input — went with the tables.

import type React from "react";
import { memo, useRef, useState } from "react";
import { parseNumericInput } from "../../units";
import { offerDatalist } from "./editorTable";

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

  // Strict parse via parseNumericInput: rejects interleaved garbage that
  // parseFloat would prefix-salvage ("8F.6G2Y" → 8), while tolerating a
  // pasted display unit ("8.62 m" → 8.62).
  function validate(raw: string): { err: string | null; normalized: string } {
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
      <input
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
  if (!offerDatalist(options.length)) return null;
  return (
    <datalist id={id}>
      {options.map((opt) => (
        <option key={opt} value={opt} />
      ))}
    </datalist>
  );
});
