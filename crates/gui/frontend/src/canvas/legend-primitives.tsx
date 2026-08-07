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
  // Wide enough for the scale control's three options on one row at the
  // 10px label size (~177px of buttons inside a 28px padding box). At 200
  // they overflowed by a few pixels and wrapped mid-label — "Whole run"
  // broke across two lines.
  width: 224,
  display: "flex",
  flexDirection: "column",
  gap: 12,
};

/**
 * The colour-scale toggle at the left end of the control bar: a stack of
 * mini ramp swatches.
 *
 * The swatches are the affordance — they are a preview of what the popover
 * explains, and they change with the selected variables — so no chevron is
 * needed to advertise that there is more behind them.
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
  padding: "5px 9px 5px 11px",
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
// small ring for areal regions. All use "var(--text-secondary)" so they track
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
                  o.value === value
                    ? "var(--selection-bg-strong)"
                    : "transparent",
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

/**
 * What a ramp is scaled against.
 *
 * These are three answers to one question, not two independent settings,
 * which is why they share a control: `criteria` pins the scale to the
 * project's threshold bands and ignores the data range entirely, so it
 * cannot meaningfully combine with either data-derived range.
 */
export type ScaleMode = "run" | "step" | "criteria";

export interface ScaleOption {
  mode: ScaleMode;
  label: string;
  tip: string;
}

/** The two data-derived scales, offered for every variable. */
export const DATA_SCALE_OPTIONS: readonly ScaleOption[] = [
  {
    mode: "run",
    label: "Whole run",
    tip: "One scale for every step: colours compare across time",
  },
  {
    mode: "step",
    label: "This step",
    tip: "Rescale each step: the pattern within a moment reads fully",
  },
];

/** Offered only for variables the project has criteria bands for. */
export const CRITERIA_SCALE_OPTION: ScaleOption = {
  mode: "criteria",
  label: "Criteria",
  tip: "Pin the scale to the project's threshold bands",
};

/**
 * Segmented control selecting what the ramps above it are scaled against.
 *
 * It belongs in the legend popover because the legend *is* the scale — and
 * because the min/max numbers directly above change as you scrub once
 * "This step" is on, which explains the control better than a tooltip
 * could.
 */
export function ScaleControl({
  value,
  options,
  onChange,
  onEditCriteria,
}: {
  value: ScaleMode;
  options: readonly ScaleOption[];
  onChange: (mode: ScaleMode) => void;
  /** Opens the criteria editor. Omitted where there is nothing to edit. */
  onEditCriteria?: () => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "flex-start",
        gap: 6,
        marginTop: 8,
        paddingTop: 8,
        borderTop: "1px solid var(--border)",
      }}
    >
      <div
        style={{
          display: "flex",
          // Wrap the buttons as whole units if a future label or translation
          // outgrows the row: a second row of intact options is readable,
          // a broken word is not.
          flexWrap: "wrap",
          gap: 3,
          flex: 1,
          minWidth: 0,
        }}
      >
        {options.map(({ mode, label, tip }) => (
          <button
            type="button"
            key={mode}
            onClick={() => onChange(mode)}
            data-tooltip={tip}
            style={{
              flex: 1,
              padding: "3px 6px",
              borderRadius: 5,
              border: "1px solid",
              borderColor:
                value === mode ? "var(--selection-border)" : "transparent",
              background: value === mode ? "var(--accent-dim)" : "transparent",
              color: value === mode ? "var(--accent)" : "var(--text-tertiary)",
              fontSize: "var(--text-xs)",
              fontFamily: "var(--font-ui)",
              whiteSpace: "nowrap",
              cursor: "pointer",
            }}
          >
            {label}
          </button>
        ))}
      </div>
      {/* Outside the group and set apart from it. Flush against a segmented
          control this would read as a fourth scale, and it is an action:
          the others change what the colours mean, this changes the ruler
          they are measured against. */}
      {onEditCriteria && (
        <button
          type="button"
          className="tool-btn"
          onClick={onEditCriteria}
          aria-label="Edit criteria"
          data-tooltip="Edit the criteria these bands come from"
          style={{
            flex: "0 0 auto",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            padding: 4,
            borderRadius: 5,
            border: "1px solid transparent",
            background: "transparent",
            color: "var(--text-tertiary)",
            cursor: "pointer",
          }}
        >
          {/* Sliders: the bands are cut points someone sets, and this is
              where they are set. */}
          <svg
            width="13"
            height="13"
            viewBox="0 0 14 14"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
            aria-hidden="true"
            focusable="false"
          >
            <title>Edit criteria</title>
            <path d="M2 4h10M2 10h10" />
            <circle cx="5" cy="4" r="1.6" fill="currentColor" stroke="none" />
            <circle
              cx="9.5"
              cy="10"
              r="1.6"
              fill="currentColor"
              stroke="none"
            />
          </svg>
        </button>
      )}
    </div>
  );
}

/**
 * Discrete swatches for a categorical variable — one labelled chip per
 * engine-declared state, in place of a gradient.
 *
 * A closed set of states has no "between", so drawing it as a bar would
 * invite reading a magnitude off colours that only carry identity.
 */
export function CategorySwatches({
  items,
}: {
  items: readonly { label: string; color: string }[];
}) {
  return (
    <div style={{ display: "flex", flexWrap: "wrap", gap: "4px 10px" }}>
      {items.map(({ label, color }) => (
        <div
          key={label}
          style={{ display: "flex", alignItems: "center", gap: 5 }}
        >
          <div
            style={{
              width: 10,
              height: 10,
              borderRadius: 3,
              background: color,
              flexShrink: 0,
            }}
          />
          <span
            style={{
              fontSize: "var(--text-xs)",
              color: "var(--text-tertiary)",
            }}
          >
            {label}
          </span>
        </div>
      ))}
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
