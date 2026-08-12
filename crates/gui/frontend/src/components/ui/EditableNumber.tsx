// ── An in-place numeric field ─────────────────────────────────────────────────
//
// The single input behind every place a model number is edited: the
// inspector's Properties rows and the Editor's per-kind tables. One
// component rather than one per surface, because the rules that make an
// edit safe are the same wherever the number is shown, and two copies of
// them drift — the inspector already had its own before the tables
// existed.
//
// It knows nothing about elements or engines. It is given a number in the
// unit the backend serves and hands one back in the same unit; the
// conversion to and from what the user reads is `editableNumberText` and
// `parseElementAttribute`, which are defined together so they cannot
// disagree.

import { useEffect, useState } from "react";
import {
  type ElementAttributeQuantity,
  editableNumberText,
  parseElementAttribute,
} from "../../hooks";

/**
 * An input that commits a number on blur or Enter and abandons it on
 * Escape.
 *
 * The three rules that make it safe to type in, all of them earned:
 *
 * - **An untouched field never writes.** A commit whose draft still
 *   matches what was displayed does nothing, so blurring a field you
 *   only looked at cannot store a rounded version of what was there.
 *   The display round trip is lossy by design — 1 m reads as 3.28 ft
 *   and returns 0.99974 m — and this is what keeps that loss confined
 *   to values the user deliberately changed.
 * - **A half-typed value is no value.** "-" and "1e" on the way to a
 *   number parse to nothing rather than to zero, and leave the model
 *   alone.
 * - **A refused write restores what was there.** `onCommit` rejecting
 *   returns the field to the value the model still holds, rather than
 *   leaving a number on screen that was never stored.
 *
 * `onCommit` reporting the failure is the caller's business — this
 * component has no opinion about toasts, and taking one would tie it to
 * app state it does not otherwise need.
 */
export function EditableNumber({
  value,
  quantity,
  sys,
  label,
  width = 72,
  align = "right",
  chrome = "boxed",
  onCommit,
}: {
  /** In the unit the backend serves — SI for a quantity-bearing value. */
  value: number;
  quantity?: ElementAttributeQuantity;
  sys: "si" | "us";
  /** Accessible name; the visible label lives beside the field. */
  label: string;
  width?: number | string;
  align?: "left" | "right";
  /**
   * How the field is drawn.
   *
   * `boxed` is a field on a panel — a bordered input sized to its
   * content, which is what a Properties row and a create dialog want.
   * `cell` is a field that *is* a table cell: it fills the cell, carries
   * the cell's own padding, and shows no chrome until focused, so a
   * column of them reads as a column of values rather than a column of
   * inputs. The padding is the shared `EDITOR_TD`'s, so an editable row
   * is exactly as tall as a read-only one — a virtualised table
   * estimates one height for every row and scrolls wrong if they differ.
   */
  chrome?: "boxed" | "cell";
  /** Given the new value in the same unit as `value`. */
  onCommit: (value: number) => Promise<void> | void;
}) {
  const shown = editableNumberText(value, quantity, sys);
  const [draft, setDraft] = useState(shown);
  const [saving, setSaving] = useState(false);
  // Only the cell presentation needs this: a field with no chrome at
  // rest has to grow some while it is being typed in, or there is
  // nothing on screen saying which cell the keystrokes are going to.
  const [focused, setFocused] = useState(false);
  // The field redraws from a refetch after every write, and the unit
  // system can change under it; the draft follows the value it is
  // editing rather than stranding the user on a stale one.
  useEffect(() => setDraft(shown), [shown]);

  const commit = () => {
    setFocused(false);
    if (draft === shown || saving) return;
    const next = parseElementAttribute(draft, quantity, sys);
    if (next == null) {
      setDraft(shown);
      return;
    }
    setSaving(true);
    Promise.resolve(onCommit(next))
      .catch(() => setDraft(shown))
      .finally(() => setSaving(false));
  };

  return (
    <input
      value={draft}
      onChange={(e: React.ChangeEvent<HTMLInputElement>) =>
        setDraft(e.target.value)
      }
      // A click into a cell is a click into that cell, not onto the row
      // beneath it — the row's own handler selects, and selecting is a
      // different intent from typing. The water-distribution cells do
      // the same.
      onClick={(e) => e.stopPropagation()}
      onFocus={() => setFocused(true)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") {
          setDraft(shown);
          e.currentTarget.blur();
        }
      }}
      disabled={saving}
      aria-label={label}
      style={
        chrome === "cell"
          ? {
              display: "block",
              width: "100%",
              boxSizing: "border-box",
              padding: "7px 10px",
              background: focused
                ? "var(--bg-input, rgba(255,255,255,0.05))"
                : saving
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
              textAlign: align,
            }
          : {
              width,
              textAlign: align,
              background: "var(--surface-2)",
              border: "1px solid var(--border)",
              borderRadius: 4,
              color: "inherit",
              font: "inherit",
              padding: "1px 4px",
            }
      }
    />
  );
}
