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

/** One class per intent. The shared `.dlg-btn` carries the shape; these
 * carry the colour and, with it, the hover and disabled states — which
 * is why they are classes: neither can be written inline. */
const INTENT_CLASS: Record<DialogIntent, string> = {
  primary: "dlg-btn-primary",
  secondary: "dlg-btn-secondary",
  danger: "dlg-btn-danger",
};

export function DialogButton({
  intent = "secondary",
  disabled,
  className,
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
      className={`dlg-btn ${INTENT_CLASS[intent]}${className ? ` ${className}` : ""}`}
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
