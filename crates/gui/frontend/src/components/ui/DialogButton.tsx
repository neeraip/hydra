// ── The buttons a dialog ends with ───────────────────────────────────────────
//
// Every modal in this app used to draw its own. There were at least six,
// each a hand-written `<button>` with its own padding, radius, weight and
// hover — 6px versus 7px, radius 5 versus 6, a confirm that was solid
// accent in one dialog and an outlined `tool-btn` in another. None of it
// was a decision; each was written next to whatever the author had open.
//
// The same drift the element badges had before `TypeBadge`, and the same
// answer: one renderer, three intents, no inline styles at the call site.
// `dialogButtonsAreShared` in the modal test pins it.

import type React from "react";

/**
 * What the button is *for*, not what it looks like.
 *
 * `primary` commits what the dialog was opened to do. `secondary`
 * leaves without doing it. `danger` commits something that cannot be
 * undone — a colour rather than a confirmation step, because the
 * confirmation is the dialog.
 */
export type DialogIntent = "primary" | "secondary" | "danger";

const INTENT: Record<DialogIntent, React.CSSProperties> = {
  primary: {
    background: "var(--accent)",
    border: "1px solid var(--accent)",
    color: "var(--accent-fg)",
    fontWeight: 600,
  },
  secondary: {
    background: "transparent",
    border: "1px solid var(--border)",
    color: "var(--text-secondary)",
    fontWeight: 500,
  },
  danger: {
    background: "var(--color-danger, #ef4444)",
    border: "1px solid var(--color-danger, #ef4444)",
    color: "#fff",
    fontWeight: 600,
  },
};

/** Disabled is drawn once, here: a dimmed version of whatever it is,
 * rather than each dialog inventing a grey. */
const DISABLED: React.CSSProperties = {
  background: "var(--bg-card)",
  border: "1px solid var(--border)",
  color: "var(--text-disabled)",
  cursor: "not-allowed",
  opacity: 0.6,
};

export function DialogButton({
  intent = "secondary",
  disabled,
  style,
  children,
  // Forwarded, because a confirmation dialog focuses its Cancel on
  // open: a stray Enter must never commit a destructive action.
  ref,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement> & {
  intent?: DialogIntent;
  ref?: React.Ref<HTMLButtonElement>;
}) {
  return (
    <button
      type="button"
      ref={ref}
      disabled={disabled}
      style={{
        borderRadius: 6,
        padding: "6px 14px",
        fontSize: "var(--text-md)",
        fontFamily: "var(--font-ui)",
        cursor: "pointer",
        ...(disabled ? DISABLED : INTENT[intent]),
        ...style,
      }}
      {...props}
    >
      {children}
    </button>
  );
}

/** The row they sit in: right-aligned, cancel first so the destructive
 * one is never under the cursor that opened the dialog. */
export function DialogActions({ children }: { children: React.ReactNode }) {
  return (
    <div style={{ display: "flex", gap: 8, justifyContent: "flex-end" }}>
      {children}
    </div>
  );
}
