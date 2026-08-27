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

const PICKER_LIST_STYLE: CSSProperties = {
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
  padding: "5px 6px 5px 7px",
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

/**
 * Motion on the canvas: marks travelling along a link.
 *
 * Not a play triangle, which this button used to carry. That glyph
 * already means three other things in the app — run the simulation, run
 * this scenario, and play the timeline — and the first two start
 * something running while this one only changes how the picture is
 * drawn. What it turns on is a wave, hard marks or soft parcels moving
 * along the pipes, so the button shows a miniature of that, the way the
 * blend toggle beside it shows a miniature of its own gradient.
 *
 * Takes `currentColor` so the toggle's own on/off colour carries it.
 */
export function MotionGlyph() {
  return (
    <svg
      width={12}
      height={12}
      viewBox="0 0 12 12"
      aria-hidden="true"
      style={{ flexShrink: 0 }}
    >
      {/* The link the motion runs along, held back so the marks read
          first. */}
      <path
        d="M1 6h10"
        fill="none"
        stroke="currentColor"
        strokeWidth={1}
        strokeLinecap="round"
        opacity={0.4}
      />
      <path
        d="M3.1 3.7 5.4 6 3.1 8.3M6.9 3.7 9.2 6 6.9 8.3"
        fill="none"
        stroke="currentColor"
        strokeWidth={1.6}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

/** The 2D overland surface: a mesh triangle. */
export function SurfaceGlyph() {
  return (
    <svg
      width={10}
      height={10}
      viewBox="0 0 10 10"
      aria-hidden="true"
      style={{ flexShrink: 0 }}
    >
      <path
        d="M5 1.5 L9 8.5 L1 8.5 Z"
        fill="none"
        stroke="var(--text-secondary)"
        strokeWidth={1.5}
        strokeLinejoin="round"
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
              ? `${pickerLabel}, animating`
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
 * What range a ramp spans.
 *
 * Two answers to one question. Judging against criteria used to be a third
 * — the reasoning being that thresholds ignore the data range, so they
 * cannot combine with either answer to it. True of the *judged* variable
 * and false of the map: a legend shows several classes at once, and with
 * depth on nodes and velocity on links, "rescale to this step" and "judge
 * velocity" are both wanted at once and were not expressible. Criteria is
 * a separate toggle now, and this asks only about the range.
 *
 * The prefs history reads the same way from the other side: `colorMode`
 * (relative | threshold) and `rangeMode` (run | step) were once two keys,
 * merged into one three-valued mode. The merge was right that the two
 * controls overlapped and wrong that they were one question — it dropped
 * the threshold-and-step combination on the way through.
 */
export type ScaleMode = "run" | "step";

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

/** The criteria toggle's own words. Not a `ScaleOption`: it answers a
 *  different question, and it belongs to one variable rather than to the
 *  map. */
export const CRITERIA_TOGGLE = {
  label: "Criteria",
  tip: "Colour this variable by the project's threshold bands",
} as const;

/**
 * The per-variable criteria switch, shown beside a ramp whose variable has
 * thresholds to judge against.
 *
 * One per class rather than one for the map, because both engines band two
 * variables in different classes — pressure and velocity, velocity and
 * capacity — and "judge the pressures, show me velocity as a magnitude" is
 * a real reading a single switch cannot express.
 *
 * A checkbox and not a segment: as a fourth rectangle beside Run and Step
 * it read as a fourth range, which is the opposite of what it is. Not the
 * app's switch either — that is the Settings vocabulary for an app-wide
 * preference, and at 36×20 it would outweigh the ramp it annotates.
 */
export function CriteriaCheckbox({
  on,
  onChange,
}: {
  on: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <label
      data-tooltip={CRITERIA_TOGGLE.tip}
      style={{
        position: "relative",
        top: "2px",
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        flexShrink: 0,
        cursor: "pointer",
        fontSize: "var(--text-xs)",
        fontFamily: "var(--font-ui)",
        color: on ? "var(--accent)" : "var(--text-tertiary)",
        whiteSpace: "nowrap",
      }}
    >
      <input
        type="checkbox"
        checked={on}
        onChange={(e) => onChange(e.target.checked)}
        style={{
          accentColor: "var(--accent)",
          width: 12,
          height: 12,
          flexShrink: 0,
          margin: 0,
          cursor: "pointer",
        }}
      />
      {CRITERIA_TOGGLE.label}
    </label>
  );
}

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
        alignItems: "center",
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
 * What a position along a ramp names.
 *
 * Two bars wear the same shape and mean different things:
 *
 * - `linear` — a sequential ramp, a diverging one (whose gradient is
 *   clipped to the same range its end labels state), and a banded one the
 *   caller is painting as a plain magnitude. A position is a value.
 * - `bands` — a banded ramp drawn in a criterion's colours. Those segments
 *   are laid out in equal widths (`hardStopGradient`), not at the
 *   thresholds they stand for, so a position names one of the regions
 *   between the cuts rather than a number. Interpolating across it would
 *   report values the bar does not carry.
 */
export type RampScale =
  | { kind: "linear"; min: number; max: number }
  | { kind: "bands"; cuts: readonly number[] };

export type RampReading =
  | { kind: "value"; value: number }
  /** A region between two cuts; `null` at an open end. */
  | { kind: "band"; from: number | null; to: number | null };

export function rampScaleOf(
  ramp: { type: string },
  min: number,
  max: number,
  /** The criterion's cut values, when the caller is painting this variable
   *  in their colours right now. */
  cuts: readonly number[] | null,
): RampScale | null {
  if (ramp.type === "categorical") return null;
  if (ramp.type === "banded" && cuts && cuts.length > 0) {
    return { kind: "bands", cuts };
  }
  if (!Number.isFinite(min) || !Number.isFinite(max)) return null;
  return max > min ? { kind: "linear", min, max } : null;
}

/** What sits at fraction `t` (0 at the left edge, 1 at the right). */
export function rampReadingAt(scale: RampScale, t: number): RampReading {
  const clamped = Math.max(0, Math.min(1, t));
  if (scale.kind === "linear") {
    return {
      kind: "value",
      value: scale.min + clamped * (scale.max - scale.min),
    };
  }
  // One more region than there are cuts, in equal widths — the same
  // layout `hardStopGradient` paints.
  const regions = scale.cuts.length + 1;
  const i = Math.min(regions - 1, Math.floor(clamped * regions));
  return {
    kind: "band",
    from: i > 0 ? scale.cuts[i - 1] : null,
    to: i < regions - 1 ? scale.cuts[i] : null,
  };
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

/** Gradient bar with its ends labelled. Numbers are formatted `toFixed(1)`;
 * pass strings when a variable needs its own precision.
 *
 * `boundaries` replaces those ends for a bar whose axis is not the data
 * range — a banded ramp, whose equal-width segments stand for the
 * criterion's regions. There the run's min and max belong to a different
 * axis entirely: a bar of drainage velocity bands sat under "0.00" and
 * "8.599" while the hover readout said "≥ 9.843", all three correct and
 * describing two different things.
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
  boundaries,
  animating = false,
  readAt,
}: {
  gradient: string;
  min: number | string;
  max: number | string;
  /** Labels at fractions along the bar, instead of its two ends. */
  boundaries?: readonly { at: number; label: string }[];
  animating?: boolean;
  /** Formats the value at fraction `t` along the bar, or answers null
   *  where a position does not name one. Supplied by the caller because
   *  the units, the display system and the ramp's own scale are all
   *  its knowledge — see `rampScaleOf`. Absent means no readout. */
  readAt?: (t: number) => string | null;
}) {
  const label = (v: number | string) =>
    typeof v === "number" ? v.toFixed(1) : v;
  const [reading, setReading] = useState<{ t: number; text: string } | null>(
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
    setReading(text === null ? null : { t: t ?? 0, text });
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
            // Always to the right of the pointer, never centred and never
            // flipped. Centred, the chip ran off the left of the canvas at
            // the start of the bar, since the legend floats near that edge
            // with nothing to clamp against. Flipping at the halfway mark
            // fixed the overflow and cost more: the chip jumped sides
            // mid-drag, which reads as a glitch in the very gesture that
            // is meant to be a steady read. Overflow to the right lands on
            // the canvas, which clips nothing.
            left: `${reading.t * 100}%`,
            transform: "translateY(-4px)",
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
      {boundaries ? (
        // Centred on the seam each one marks, so a number sits where the
        // colour actually changes rather than at an end that means
        // nothing on this bar.
        <div style={{ position: "relative", height: "1.2em" }}>
          {boundaries.map((b) => (
            <span
              key={b.label}
              className="mono"
              style={{
                position: "absolute",
                left: `${b.at * 100}%`,
                transform: "translateX(-50%)",
                fontSize: "var(--text-xs)",
                color: "var(--text-tertiary)",
                whiteSpace: "nowrap",
              }}
            >
              {b.label}
            </span>
          ))}
        </div>
      ) : (
        <div style={{ display: "flex", justifyContent: "space-between" }}>
          <span
            className="mono"
            style={{
              fontSize: "var(--text-xs)",
              color: "var(--text-tertiary)",
            }}
          >
            {label(min)}
          </span>
          <span
            className="mono"
            style={{
              fontSize: "var(--text-xs)",
              color: "var(--text-tertiary)",
            }}
          >
            {label(max)}
          </span>
        </div>
      )}
    </div>
  );
}

/**
 * A toggle in the control bar: on reads as filled, off as empty.
 *
 * One component for every toggle in the bar because two of them wrote
 * this themselves and drifted — the animation button filled when on
 * while the surface's blend button only changed the icon's colour, so
 * the same state was drawn two ways in the same bar, and "off" and
 * "unavailable" became the same picture on the one that dimmed.
 *
 * Hover is stacked over whichever state fill is showing rather than
 * replacing it: `--selection-bg-strong` is three times the weight of
 * `--nav-hover`, so swapping one for the other would *dim* a lit button
 * under the pointer. Inline styles beat the stylesheet, so `.tool-btn:hover`
 * could never show through the state fill and the handlers do it here.
 */
export function BarToggle({
  active,
  disabled = false,
  label,
  tooltip,
  onClick,
  style,
  children,
}: {
  /** Whether the thing this toggles is on. */
  active: boolean;
  /** Refused, and saying so: dimmed, uncoloured, and inert. */
  disabled?: boolean;
  /** Accessible name. */
  label: string;
  /** Hover text, which usually names the action rather than the state. */
  tooltip?: string;
  onClick: () => void;
  /** Per-button geometry only (padding, corner radius) — the state
   * colours are this component's, and passing them here would be the
   * drift it exists to prevent. */
  style?: CSSProperties;
  children: ReactNode;
}) {
  const fill =
    active && !disabled ? "var(--selection-bg-strong)" : "transparent";
  const hoverFill = `linear-gradient(var(--nav-hover), var(--nav-hover)), ${fill}`;
  return (
    <button
      type="button"
      className="tool-btn"
      disabled={disabled}
      aria-pressed={active}
      aria-label={label}
      data-tooltip={tooltip ?? label}
      data-tooltip-pos="top"
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
      onMouseEnter={(e) => {
        if (disabled) return;
        e.currentTarget.style.background = hoverFill;
        if (!active) e.currentTarget.style.color = "var(--text-primary)";
      }}
      onMouseLeave={(e) => {
        if (disabled) return;
        e.currentTarget.style.background = fill;
        if (!active) e.currentTarget.style.color = "var(--text-secondary)";
      }}
      style={{
        ...PICKER_BTN_STYLE,
        ...style,
        background: fill,
        color:
          active && !disabled
            ? "var(--accent)"
            : disabled
              ? "var(--text-tertiary)"
              : "var(--text-secondary)",
        opacity: disabled ? 0.5 : 1,
        cursor: disabled ? "default" : "pointer",
      }}
    >
      {children}
    </button>
  );
}
