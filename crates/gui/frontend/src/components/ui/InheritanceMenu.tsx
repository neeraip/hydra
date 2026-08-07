/**
 * The rows of a menu that chooses between following a setting and pinning
 * one.
 *
 * Two controls answer this shape of question — display units and the canvas
 * ground — and a third will. Each shows a Default group whose single row
 * names what the setting currently resolves to, then an Override group
 * whose rows stay put when that setting moves. The rows were copied
 * verbatim between the two, differing only in a parameter name.
 *
 * What is shared here is the row and the group heading, not the structure
 * above them: which groups exist and what goes in them is the question each
 * control is asking, and folding that in would mean a component with a
 * parameter per caller.
 */

import type { ReactNode } from "react";

export function MenuRow({
  label,
  description,
  selected,
  onSelect,
}: {
  label: string;
  /** What choosing this row does. Carries the whole difference between the
   *  Default row and an override that happens to read the same. */
  description?: string;
  selected: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      role="menuitemradio"
      aria-checked={selected}
      onClick={onSelect}
      // Restores to the *selected* colour rather than a constant: the
      // checked row is accent-coloured, and resetting every row to
      // secondary on mouse-out would quietly un-highlight the one in force.
      onMouseEnter={(e) => {
        e.currentTarget.style.background = "var(--nav-hover)";
        e.currentTarget.style.color = "var(--text-primary)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "transparent";
        e.currentTarget.style.color = selected
          ? "var(--accent)"
          : "var(--text-secondary)";
      }}
      style={{
        display: "flex",
        alignItems: "flex-start",
        gap: 8,
        width: "100%",
        padding: "5px 10px",
        border: "none",
        background: "transparent",
        color: selected ? "var(--accent)" : "var(--text-secondary)",
        fontFamily: "var(--font-ui)",
        fontSize: "var(--text-md)",
        fontWeight: selected ? 500 : 400,
        cursor: "pointer",
        textAlign: "left",
        transition: "background var(--t-fast), color var(--t-fast)",
      }}
    >
      <span style={{ width: 12, flexShrink: 0 }}>{selected ? "✓" : ""}</span>
      <span
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 1,
          // Without this a flex child refuses to shrink below its content,
          // so the description stretches the menu instead of wrapping.
          minWidth: 0,
        }}
      >
        {label}
        {description && (
          <span
            style={{
              fontSize: "var(--text-xs)",
              fontWeight: 400,
              color: "var(--text-tertiary)",
              lineHeight: 1.4,
            }}
          >
            {description}
          </span>
        )}
      </span>
    </button>
  );
}

/**
 * A group heading, optionally with a hint about what the whole group means.
 *
 * A Default group of one row carries its explanation on the row, where it
 * describes what choosing that row does. An Override group has several rows
 * and one shared consequence, which belongs up here rather than repeated on
 * each of them.
 */
export function MenuGroupLabel({
  children,
  hint,
}: {
  children: ReactNode;
  hint?: string;
}) {
  return (
    <div
      style={{
        padding: "6px 10px 2px",
        fontSize: "var(--text-xs)",
        color: "var(--text-tertiary)",
      }}
    >
      <span style={{ letterSpacing: "0.05em", textTransform: "uppercase" }}>
        {children}
      </span>
      {hint && <span style={{ opacity: 0.85 }}> · {hint}</span>}
    </div>
  );
}

/** The divider between the two groups. */
export function MenuGroupDivider() {
  return (
    <div style={{ height: 1, margin: "4px 0", background: "var(--border)" }} />
  );
}
