// ── Legend design language ────────────────────────────────────────────────────
// The abstract legend concept, expressed as shared primitives: a persistent
// glass control bar of variable pickers (glyph + current value + chevron,
// popover list above), and an expandable details card of labelled colour
// ramps. Each engine's legend implementation composes these so every legend
// has the same look and feel; what differs per engine is only which
// variables exist and what extra affordances (thresholds, animation) make
// sense there.

import { ChevronUpDownIcon } from "@heroicons/react/16/solid";
import type { CSSProperties, ReactNode } from "react";

export const SECTION_LABEL_STYLE: CSSProperties = {
  fontSize: "var(--text-xs)",
  fontWeight: 600,
  color: "var(--text-secondary)",
  marginBottom: 5,
};

export const PICKER_BTN_STYLE: CSSProperties = {
  width: "auto",
  height: 26,
  padding: "0 8px",
  gap: 4,
  display: "flex",
  alignItems: "center",
  fontSize: "var(--text-sm)",
  fontWeight: 600,
  fontFamily: "var(--font-ui)",
  color: "var(--text-primary)",
  whiteSpace: "nowrap",
};

export const PICKER_LIST_STYLE: CSSProperties = {
  position: "absolute",
  bottom: "calc(100% + 6px)",
  left: 0,
  backdropFilter: "blur(20px) saturate(160%)",
  WebkitBackdropFilter: "blur(20px) saturate(160%)",
  borderRadius: 8,
  overflow: "hidden",
  minWidth: 130,
  zIndex: 40,
};

/** Root container: bottom-left of the canvas, above the timeline. */
export const LEGEND_ROOT_STYLE: CSSProperties = {
  position: "absolute",
  bottom: 14,
  left: "calc(var(--rail-effective-w, 0px) + 16px)",
  zIndex: 30,
  display: "flex",
  flexDirection: "column",
  alignItems: "flex-start",
  transition: "left var(--rail-transition)",
};

/** The expandable details card that opens above the control bar. */
export const LEGEND_POPOVER_STYLE: CSSProperties = {
  marginBottom: 8,
  backdropFilter: "blur(20px) saturate(160%)",
  WebkitBackdropFilter: "blur(20px) saturate(160%)",
  borderRadius: 10,
  padding: "10px 14px",
  width: 200,
  display: "flex",
  flexDirection: "column",
  gap: 12,
};

/**
 * The colour-scale toggle at the left end of the control bar: a stack of
 * mini ramp swatches plus a chevron.
 *
 * The left corners nest inside the bar's 20px rounding (20 − 4px bar
 * padding = 16), so the button's curve sits flush against the glass pill's
 * end. That large radius eats horizontally into the content box at the top
 * and bottom of the button, and the more swatches stack the taller the
 * button gets — at three ramps the outer two were clipping into the curve.
 * The left padding therefore clears the corner arc rather than matching
 * the right side, which only has a 6px radius to clear.
 */
export const LEGEND_SWATCH_BTN_STYLE: CSSProperties = {
  width: "auto",
  height: "auto",
  gap: 5,
  padding: "5px 8px 5px 11px",
  borderRadius: "16px 6px 6px 16px",
};

/** The persistent control bar (glass pill) the pickers live in. */
export const LEGEND_BAR_STYLE: CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 4,
  padding: 4,
  minHeight: 32,
  borderRadius: 20,
  backdropFilter: "blur(20px) saturate(160%)",
  WebkitBackdropFilter: "blur(20px) saturate(160%)",
};

export interface PickerOption<T extends string> {
  value: T;
  label: string;
}

// ── Picker glyphs ─────────────────────────────────────────────────────────────
// Micro-icons distinguishing the variable pickers: a filled dot matching how
// point elements render on the canvas, a short segment for polylines, and a
// small ring for areal regions. All use var(--text-secondary) so they track
// the legend's micro-label colour in every theme.

export function NodeGlyph() {
  return (
    <svg
      width={10}
      height={10}
      viewBox="0 0 10 10"
      aria-hidden="true"
      style={{ flexShrink: 0 }}
    >
      <circle cx={5} cy={5} r={3.2} fill="var(--text-secondary)" />
    </svg>
  );
}

export function LinkGlyph() {
  return (
    <svg
      width={10}
      height={10}
      viewBox="0 0 10 10"
      aria-hidden="true"
      style={{ flexShrink: 0 }}
    >
      <line
        x1={1.5}
        y1={8.5}
        x2={8.5}
        y2={1.5}
        stroke="var(--text-secondary)"
        strokeWidth={2}
        strokeLinecap="round"
      />
    </svg>
  );
}

export function RegionGlyph() {
  return (
    <svg
      width={10}
      height={10}
      viewBox="0 0 10 10"
      aria-hidden="true"
      style={{ flexShrink: 0 }}
    >
      <path
        d="M1.5 3.2 L5 1.5 L8.5 3.2 L8 8 L2 8.5 Z"
        fill="none"
        stroke="var(--text-secondary)"
        strokeWidth={1.5}
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** Always-visible dropdown button for switching a canvas variable — mirrors
 * the basemap/CRS picker pattern used in the canvas toolbar. */
export function PickerButton<T extends string>({
  value,
  options,
  isOpen,
  onToggle,
  onSelect,
  icon,
  pickerLabel,
}: {
  value: T;
  options: PickerOption<T>[];
  isOpen: boolean;
  onToggle: () => void;
  onSelect: (v: T) => void;
  /** Glyph rendered before the current value ("which picker is this?"). */
  icon?: ReactNode;
  /** Accessible name + tooltip, e.g. "Node variable". */
  pickerLabel?: string;
}) {
  const current = options.find((o) => o.value === value);
  return (
    <div style={{ position: "relative" }}>
      <button
        type="button"
        className="tool-btn"
        onClick={(e) => {
          e.stopPropagation();
          onToggle();
        }}
        aria-label={pickerLabel}
        data-tooltip={pickerLabel}
        data-tooltip-pos="top"
        style={PICKER_BTN_STYLE}
      >
        {icon}
        {current?.label ?? value}
        <ChevronUpDownIcon style={{ width: 12, height: 12 }} />
      </button>
      {isOpen && (
        <div
          className="legend-glass legend-glass--raised"
          style={PICKER_LIST_STYLE}
        >
          {options.map((o) => (
            <button
              type="button"
              key={o.value}
              onClick={() => onSelect(o.value)}
              style={{
                display: "block",
                width: "100%",
                padding: "6px 10px",
                border: "none",
                background:
                  o.value === value ? "rgba(74,144,217,0.22)" : "transparent",
                color:
                  o.value === value ? "var(--accent)" : "var(--text-secondary)",
                cursor: "pointer",
                fontSize: "var(--text-sm)",
                textAlign: "left",
                fontFamily: "var(--font-ui)",
              }}
            >
              {o.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** Gradient bar with min/max labels. Numbers are formatted `toFixed(1)`;
 * pass strings when a variable needs its own precision. */
export function Ramp({
  gradient,
  min,
  max,
}: {
  gradient: string;
  min: number | string;
  max: number | string;
}) {
  const label = (v: number | string) =>
    typeof v === "number" ? v.toFixed(1) : v;
  return (
    <div>
      <div
        style={{
          height: 10,
          borderRadius: 5,
          background: gradient,
          marginBottom: 4,
        }}
      />
      <div style={{ display: "flex", justifyContent: "space-between" }}>
        <span
          className="mono"
          style={{ fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}
        >
          {label(min)}
        </span>
        <span
          className="mono"
          style={{ fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}
        >
          {label(max)}
        </span>
      </div>
    </div>
  );
}
