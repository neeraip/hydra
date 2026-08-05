/**
 * The two shapes every settings row is built from — a section heading and a
 * labelled row with its control on the right.
 *
 * Shared so the loading skeleton can be built from the *same* pieces as the
 * real thing rather than a copy of their measurements. A skeleton whose
 * paddings and type sizes are restated somewhere else drifts from what it
 * stands in for, and the drift shows up as exactly the jump the skeleton
 * exists to prevent.
 */

import type React from "react";

export function Section({ children }: { children: React.ReactNode }) {
  return (
    <h2
      style={{
        // The element's own defaults would fight the type scale below.
        margin: 0,
        marginTop: 32,
        marginBottom: 2,
        fontSize: "var(--text-sm)",
        fontWeight: 600,
        letterSpacing: "0.08em",
        textTransform: "uppercase",
        color: "var(--text-tertiary)",
      }}
    >
      {children}
    </h2>
  );
}

export function SettingRow({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "12px 0",
        borderBottom: "1px solid var(--border)",
        gap: 24,
      }}
    >
      {/* Takes the slack, so the control lane below is what fixes the
          column boundary rather than whichever control happens to be
          widest. `min-width: 0` lets a long description wrap instead of
          forcing the row wider. */}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={{
            fontSize: "var(--text-lg)",
            color: "var(--text-primary)",
            fontWeight: 500,
          }}
        >
          {label}
        </div>
        {description && (
          <div
            style={{
              fontSize: "var(--text-md)",
              color: "var(--text-secondary)",
              marginTop: 2,
              lineHeight: 1.5,
            }}
          >
            {description}
          </div>
        )}
      </div>
      {/* A lane wide enough for the widest control, so every row's label
          column is the same width and swapping a loading placeholder for
          the real control cannot move anything — not the label, and not
          the line a description wraps at.

          Sized in `em` rather than pixels so it grows with the text scale;
          a fixed lane would clip the theme buttons at the larger steps. */}
      <div
        style={{
          flexShrink: 0,
          minWidth: "14em",
          display: "flex",
          justifyContent: "flex-end",
        }}
      >
        {children}
      </div>
    </div>
  );
}
