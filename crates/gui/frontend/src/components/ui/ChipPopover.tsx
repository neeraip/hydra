/** A chip that opens a glass panel beneath it — the same popover pattern
 * as the canvas legend's, shared by every engine's criteria control. The
 * content is the caller's; this owns opening, outside-click dismissal,
 * and Escape.
 *
 * The chip is the label alone. Its `summary` reads the whole setting back
 * on hover instead of inline: the control sits in the project toolbar,
 * where a summary would have to compete with the scenario strip for width
 * and lose, and where the tooltip costs nothing. */

import { useEffect, useRef, useState } from "react";

/** Above the canvas and the secondary rail, with the toolbar's other
 * popovers — the unit picker's menu sits here too, and a criteria panel
 * opened beside it must not be the one that loses. */
const PANEL_Z = 120;

export function ChipPopover({
  label,
  summary,
  ariaLabel,
  children,
}: {
  /** The chip's text ("Criteria"). */
  label: string;
  /** The setting read back in full, shown on hover. */
  summary?: string;
  /** Dialog name for assistive tech; defaults to `label`. */
  ariaLabel?: string;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  // Where to draw the panel, measured from the chip when it opens.
  //
  // Fixed rather than absolute, for the same reason the unit picker's menu
  // is: `ProjectPage` wraps the toolbar in a column with `overflow:
  // hidden`, which clips any child extending past the toolbar's own height
  // — so an absolutely-positioned panel is cut off however high its
  // z-index goes. Fixed positioning leaves that clipping context entirely.
  const [anchor, setAnchor] = useState<{ right: number; top: number } | null>(
    null,
  );

  // Escape closes, from anywhere — the popover holds inputs, and a
  // keyboard user mid-field should not have to reach for the mouse.
  //
  // The anchor is measured, not tracked: the toolbar does not scroll, so
  // the only things that can move it are a resize or a layout change big
  // enough that closing is the honest response.
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    const close = () => setOpen(false);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", close);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", close);
    };
  }, [open]);

  return (
    <span style={{ display: "inline-flex" }}>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => {
          const rect = triggerRef.current?.getBoundingClientRect();
          if (rect) {
            // Right-aligned to the chip: the control sits near the end of
            // the toolbar, and a wide panel hung from its left edge would
            // run off the window.
            setAnchor({
              right: Math.max(8, window.innerWidth - rect.right),
              top: rect.bottom + 6,
            });
          }
          setOpen((o) => !o);
        }}
        aria-expanded={open}
        aria-haspopup="dialog"
        data-tooltip={summary ?? `Edit ${label.toLowerCase()}`}
        data-tooltip-pos="bottom"
        // The toolbar's button language, shared with the unit picker beside
        // it: background, border and text lift together, and an open panel
        // keeps the lit state so the trigger reads as the thing the panel
        // belongs to.
        onMouseEnter={(e) => {
          e.currentTarget.style.background = "var(--nav-hover)";
          e.currentTarget.style.borderColor = "var(--border-hover)";
          e.currentTarget.style.color = "var(--text-primary)";
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.background = open
            ? "var(--nav-hover)"
            : "transparent";
          e.currentTarget.style.borderColor = "var(--border)";
          e.currentTarget.style.color = "var(--text-secondary)";
        }}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 6,
          padding: "3px 8px",
          borderRadius: 6,
          border: "1px solid var(--border)",
          background: open ? "var(--nav-hover)" : "transparent",
          color: "var(--text-secondary)",
          fontFamily: "var(--font-ui)",
          fontSize: "var(--text-sm)",
          whiteSpace: "nowrap",
          cursor: "pointer",
          transition:
            "background var(--t-fast), border-color var(--t-fast), color var(--t-fast)",
        }}
      >
        {label}
      </button>
      {open && anchor && (
        <>
          {/* Transparent backdrop: outside clicks dismiss without dimming
              the page (the legend popover's deliberate non-modality). Just
              under the panel, so a click anywhere else closes rather than
              reaching the control beneath. */}
          {/* biome-ignore lint/a11y/noStaticElementInteractions: backdrop closes the popover on pointer interaction; Escape covers the keyboard. */}
          {/* biome-ignore lint/a11y/useKeyWithClickEvents: same — the keyboard path is the Escape handler above. */}
          <div
            style={{ position: "fixed", inset: 0, zIndex: PANEL_Z - 1 }}
            onClick={() => setOpen(false)}
          />
          <div
            className="legend-glass legend-glass--raised"
            role="dialog"
            aria-label={ariaLabel ?? label}
            style={{
              position: "fixed",
              top: anchor.top,
              right: anchor.right,
              zIndex: PANEL_Z,
              width: 520,
              maxWidth: "calc(100vw - 16px)",
              padding: 14,
              borderRadius: 10,
            }}
          >
            {children}
          </div>
        </>
      )}
    </span>
  );
}
