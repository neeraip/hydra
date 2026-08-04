import {
  ArrowsRightLeftIcon,
  ArrowsUpDownIcon,
} from "@heroicons/react/16/solid";
import { useCallback, useRef } from "react";
import {
  ASPECT_SLIDER_DEFAULT,
  ASPECT_SLIDER_MAX,
  ASPECT_SLIDER_MIN,
  aspectFactor,
  clampSliderValue,
  sliderValueFromPointer,
  thumbOffsetPercent,
} from "../../../canvas/schematicAspect";

const TRACK_HEIGHT = 170;
/** Arrow-key increment. 2 units is ~5% on each axis. */
const KEY_STEP = 2;
const GLYPH = { width: 10, height: 10 } as const;

/**
 * Aspect control for the schematic layout: drag up to spread the layers apart
 * and tighten each layer, drag down for the reverse.
 *
 * One slider rather than one per axis. The two spacings' *uniform* component is
 * just a zoom — and the camera fit divides it out anyway — so two independent
 * tracks shared a single visible degree of freedom and behaved as one control
 * that reversed direction partway along. Trading the axes against each other
 * keeps their product at 1, which leaves exactly the reshape zoom cannot do.
 *
 * Hand-built rather than an `<input type="range">` turned upright: a vertical
 * range input needs `-webkit-appearance: slider-vertical` or `writing-mode:
 * vertical-rl` depending on engine and version, this ships on both WKWebView
 * and WebView2, and the custom thumb styling survives neither reliably.
 */
export function SchematicAspectSlider({
  value,
  onChange,
}: {
  value: number;
  onChange: (next: number) => void;
}) {
  const trackRef = useRef<HTMLDivElement>(null);
  // Coalesce pointer moves to one update per frame: each change re-lays out the
  // network, ~12ms at 46k elements, so a raw pointermove stream would queue
  // several of those per frame and the drag would visibly lag.
  const pendingRef = useRef<number | null>(null);
  const rafRef = useRef<number | null>(null);

  const commit = useCallback(
    (next: number) => {
      pendingRef.current = next;
      if (rafRef.current != null) return;
      rafRef.current = requestAnimationFrame(() => {
        rafRef.current = null;
        const queued = pendingRef.current;
        pendingRef.current = null;
        if (queued != null) onChange(queued);
      });
    },
    [onChange],
  );

  const valueFromEvent = useCallback((clientY: number): number | null => {
    const track = trackRef.current;
    if (!track) return null;
    const rect = track.getBoundingClientRect();
    return sliderValueFromPointer(clientY, rect.top, rect.height);
  }, []);

  const handlePointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      // Pointer capture, so a drag that slips off the narrow track keeps working
      // instead of stopping the moment the cursor moves sideways.
      e.currentTarget.setPointerCapture(e.pointerId);
      const next = valueFromEvent(e.clientY);
      if (next != null) commit(next);
    },
    [commit, valueFromEvent],
  );

  const handlePointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!e.currentTarget.hasPointerCapture(e.pointerId)) return;
      const next = valueFromEvent(e.clientY);
      if (next != null) commit(next);
    },
    [commit, valueFromEvent],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      const step =
        e.key === "ArrowUp"
          ? KEY_STEP
          : e.key === "ArrowDown"
            ? -KEY_STEP
            : e.key === "PageUp"
              ? KEY_STEP * 5
              : e.key === "PageDown"
                ? -KEY_STEP * 5
                : 0;
      if (step !== 0) {
        e.preventDefault();
        onChange(clampSliderValue(value + step));
        return;
      }
      if (e.key === "Home") {
        e.preventDefault();
        onChange(ASPECT_SLIDER_MAX);
      } else if (e.key === "End") {
        e.preventDefault();
        onChange(ASPECT_SLIDER_MIN);
      }
    },
    [onChange, value],
  );

  const factor = aspectFactor(value);
  const shape =
    factor > 1
      ? `${factor.toFixed(2)}× wider`
      : factor < 1
        ? `${(1 / factor).toFixed(2)}× taller`
        : "balanced";

  return (
    <div
      className="canvas-toolbar"
      // Horizontal padding matches `.canvas-toolbar`'s own 4px so this box and
      // the viewport-control box below come out the same width; the track picks
      // up `--tool-btn-size`, which is what sets that box's content width.
      style={{ flexDirection: "column", gap: 4, padding: "8px 4px" }}
    >
      {/* End glyphs carry the direction: spreading horizontally at the top,
          vertically at the bottom. A vertical slider whose "up" means "wider"
          needs saying, and an icon says it without a label. */}
      <ArrowsRightLeftIcon
        aria-hidden
        style={{ ...GLYPH, color: "var(--text-tertiary)" }}
      />
      <div
        ref={trackRef}
        role="slider"
        tabIndex={0}
        aria-label="Schematic layout aspect"
        aria-valuemin={ASPECT_SLIDER_MIN}
        aria-valuemax={ASPECT_SLIDER_MAX}
        aria-valuenow={Math.round(value)}
        aria-valuetext={shape}
        data-tooltip={`Layout aspect — ${shape}. Drag up to spread layers, down to spread within layers${value === ASPECT_SLIDER_DEFAULT ? "" : " (double-click to reset)"}`}
        data-tooltip-pos="left"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onDoubleClick={() => onChange(ASPECT_SLIDER_DEFAULT)}
        onKeyDown={handleKeyDown}
        style={{
          position: "relative",
          width: "var(--tool-btn-size)",
          height: TRACK_HEIGHT,
          cursor: "ns-resize",
          touchAction: "none",
          display: "flex",
          justifyContent: "center",
          outline: "none",
        }}
      >
        <div
          style={{
            width: 4,
            height: "100%",
            borderRadius: 2,
            background: "var(--border-hover)",
          }}
        />
        {/* The neutral ratio — the one landmark worth being able to aim for. */}
        <div
          aria-hidden
          style={{
            position: "absolute",
            left: 3,
            right: 3,
            top: "50%",
            height: 1,
            background: "var(--text-tertiary)",
          }}
        />
        <div
          aria-hidden
          style={{
            position: "absolute",
            left: "50%",
            bottom: `${thumbOffsetPercent(value)}%`,
            width: 12,
            height: 12,
            marginLeft: -6,
            marginBottom: -6,
            borderRadius: "50%",
            background: "var(--accent)",
            boxShadow:
              "0 0 0 3px rgba(205, 211, 223, 0.22), 0 1px 4px rgba(0, 0, 0, 0.45)",
            pointerEvents: "none",
          }}
        />
      </div>
      <ArrowsUpDownIcon
        aria-hidden
        style={{ ...GLYPH, color: "var(--text-tertiary)" }}
      />
    </div>
  );
}
