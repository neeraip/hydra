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
import { useState } from "react";

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

/** Root container: bottom-left of the canvas, above the timeline.
 *
 * Transparent to the pointer, and the parts that are not are marked so
 * individually. Hit testing works on a box, not on painted pixels: this
 * one shrink-wraps around both the bar and the popover, so with the
 * popover open the empty rectangle beside the narrower card was still
 * swallowing every click, drag and hover meant for the canvas behind it. */
export const LEGEND_ROOT_STYLE: CSSProperties = {
  position: "absolute",
  bottom: 14,
  left: "calc(var(--rail-effective-w, 0px) + 16px)",
  zIndex: 30,
  display: "flex",
  flexDirection: "column",
  alignItems: "flex-start",
  transition: "left var(--rail-transition)",
  pointerEvents: "none",
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
  // Its own box is real; the root's is not (see LEGEND_ROOT_STYLE).
  pointerEvents: "auto",
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
  padding: "5px 6px 5px 8px",
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
  // As the popover: the bar is solid to the pointer even though the root
  // it sits in is not. Picker lists are descendants of this, so they are
  // re-enabled with it however far outside its box they are drawn.
  pointerEvents: "auto",
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
  animating = false,
  dimmed = false,
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
  /** Whether this class's current selection is moving on the canvas right
   * now. Pulses the glyph — the answer to "is animation doing anything
   * here", which the animation toggle deliberately no longer answers. */
  animating?: boolean;
  /** Whether the class this picker colours is hidden on the canvas. Dims
   * the button — a variable chosen for elements nobody can see is still a
   * real choice, worth keeping and worth showing as inert. The list it
   * opens stays at full strength: those rows are as clickable as ever, and
   * dimming them would say the choice is unavailable. */
  dimmed?: boolean;
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
        data-tooltip={
          dimmed
            ? `${pickerLabel} (hidden on the canvas)`
            : animating
              ? `${pickerLabel} — animating`
              : (pickerLabel ?? "")
        }
        data-tooltip-pos="top"
        // Only the button dims; the list it opens is a sibling, so those
        // rows keep their own strength.
        style={
          dimmed ? { ...PICKER_BTN_STYLE, opacity: 0.45 } : PICKER_BTN_STYLE
        }
      >
        <span
          className={animating ? "legend-anim-pulse" : undefined}
          style={{ display: "inline-flex", alignItems: "center" }}
        >
          {icon}
        </span>
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
              // Styled by class, not inline: an inline background wins
              // against the stylesheet, so a `:hover` rule could never
              // show through it.
              className={
                o.value === value
                  ? "legend-picker-option legend-picker-option--selected"
                  : "legend-picker-option"
              }
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
    label: "Run",
    tip: "One scale for every step: colours compare across time",
  },
  {
    mode: "step",
    label: "Step",
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
}: {
  value: ScaleMode;
  options: readonly ScaleOption[];
  onChange: (mode: ScaleMode) => void;
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

/**
 * How a position along a ramp maps to a value — or that it does not.
 *
 * Three different bars wear the same shape, and only one of them is the
 * obvious linear scale:
 *
 * - `linear` — a sequential ramp, and a banded one the caller is painting
 *   as a magnitude: the bar runs from `min` to `max`.
 * - `symmetric` — a diverging ramp. The gradient is built over −1…+1 and
 *   the map scales values by `max(|min|, |max|)`, so the bar is centred on
 *   zero, and reading it as `min`…`max` would be wrong wherever the two are
 *   not mirror images — which is most runs.
 * - `null` — a banded ramp drawn in a criterion's colours. Those segments
 *   are laid out in equal widths (`hardStopGradient`), not at the
 *   thresholds they stand for, so a position names a band and not a value.
 *   Reporting a number there would be inventing one.
 */
export type RampScale =
  | { kind: "linear"; min: number; max: number }
  | { kind: "symmetric"; scale: number };

export function rampScaleOf(
  ramp: { type: string },
  min: number,
  max: number,
  /** Whether the caller is painting this variable in a criterion's band
   *  colours right now. */
  banded: boolean,
): RampScale | null {
  if (!Number.isFinite(min) || !Number.isFinite(max)) return null;
  if (ramp.type === "categorical") return null;
  if (ramp.type === "banded" && banded) return null;
  if (ramp.type === "diverging") {
    const scale = Math.max(Math.abs(min), Math.abs(max));
    return scale > 0 ? { kind: "symmetric", scale } : null;
  }
  return max > min ? { kind: "linear", min, max } : null;
}

/** The value at fraction `t` (0 at the left edge, 1 at the right). */
export function rampValueAt(scale: RampScale, t: number): number {
  const clamped = Math.max(0, Math.min(1, t));
  return scale.kind === "symmetric"
    ? scale.scale * (2 * clamped - 1)
    : scale.min + clamped * (scale.max - scale.min);
}

/**
 * Where along a bar a pointer is, as a fraction.
 *
 * A zero-width box answers null rather than dividing by zero: the bar is
 * measured from the live document, and a hover can arrive in the frame
 * before layout.
 */
export function rampFractionAt(clientX: number, rect: DOMRect): number | null {
  if (!(rect.width > 0)) return null;
  return Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
}

/** Gradient bar with min/max labels. Numbers are formatted `toFixed(1)`;
 * pass strings when a variable needs its own precision.
 *
 * `animating` sends a slow sheen along the bar while the canvas is
 * animating this variable. It travels *over* the gradient rather than
 * moving the gradient itself: the colours are the data, and sliding them
 * would have the legend show values the map does not hold.
 */
export function Ramp({
  gradient,
  min,
  max,
  animating = false,
  readAt,
}: {
  gradient: string;
  min: number | string;
  max: number | string;
  animating?: boolean;
  /** Formats the value at fraction `t` along the bar, or answers null
   *  where a position does not name one. Supplied by the caller because
   *  the units, the display system and the ramp's own scale are all
   *  its knowledge — see `rampScaleOf`. Absent means no readout. */
  readAt?: (t: number) => string | null;
}) {
  const label = (v: number | string) =>
    typeof v === "number" ? v.toFixed(1) : v;
  const [reading, setReading] = useState<{ x: number; text: string } | null>(
    null,
  );

  /** Follow the pointer along the bar, reading off the value under it.
   *  The bar is measured live rather than remembered: the popover resizes
   *  with the panel and a stale width would report the wrong value. */
  const onMove = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!readAt) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const t = rampFractionAt(e.clientX, rect);
    const text = t === null ? null : readAt(t);
    setReading(text === null ? null : { x: (t ?? 0) * rect.width, text });
  };

  return (
    <div style={{ position: "relative" }}>
      {/* Above the bar rather than under the pointer: the bar is ten
          pixels tall and a chip on top of it would cover the colour the
          reader is trying to match. */}
      {reading && (
        <div
          style={{
            position: "absolute",
            bottom: "100%",
            left: reading.x,
            transform: "translate(-50%, -4px)",
            pointerEvents: "none",
            whiteSpace: "nowrap",
            background: "var(--tooltip-bg, #1e1e2a)",
            color: "var(--tooltip-text, #e2e2ec)",
            border: "1px solid var(--border-hover)",
            borderRadius: 5,
            padding: "2px 6px",
            fontSize: "var(--text-xs)",
            fontVariantNumeric: "tabular-nums",
            boxShadow: "var(--shadow-2)",
            zIndex: 1,
          }}
        >
          {reading.text}
        </div>
      )}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: pointer-only
          enhancement of a bar whose ends are already stated in text below
          it, so nothing here is reachable only by hovering. */}
      <div
        className={`legend-ramp${animating ? " legend-ramp--animating" : ""}`}
        onMouseMove={onMove}
        onMouseLeave={() => setReading(null)}
        style={{
          height: 10,
          borderRadius: 5,
          background: gradient,
          marginBottom: 4,
          cursor: readAt ? "crosshair" : undefined,
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
