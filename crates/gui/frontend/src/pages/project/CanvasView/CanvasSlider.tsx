import { type ReactNode, useCallback, useEffect, useRef } from "react";
import {
  clampSliderValue,
  SLIDER_DEFAULT,
  SLIDER_MAX,
  SLIDER_MIN,
  sliderValueFromPointer,
  thumbOffsetPercent,
} from "../../../canvas/verticalSlider";

/**
 * Track height.
 *
 * Two of these now stack above the zoom controls, and at 170 apiece they
 * took most of the canvas's right edge. A 0–100 track does not need the
 * pixels: at 110 a single pixel of travel is under one unit, so the drag is
 * still finer than the value can resolve, and the arrow keys reach anywhere
 * regardless.
 */
const TRACK_HEIGHT = 110;
/** Arrow-key increment, in track units. */
const KEY_STEP = 2;

/**
 * A vertical slider for the canvas's control stack.
 *
 * Hand-built rather than an `<input type="range">` turned upright: a
 * vertical range input needs `-webkit-appearance: slider-vertical` or
 * `writing-mode: vertical-rl` depending on engine and version, this ships
 * on both WKWebView and WebView2, and the custom thumb styling survives
 * neither reliably.
 *
 * Generic because there are two of these — layout aspect and node size —
 * and the pointer capture, the frame coalescing and the keyboard handling
 * are the same work in both. What differs is only what the number means,
 * which arrives as the glyphs, the label and the readout.
 */
export function CanvasSlider({
  value,
  onChange,
  label,
  readout,
  hint,
  topGlyph,
  bottomGlyph,
}: {
  value: number;
  onChange: (next: number) => void;
  /** Accessible name for the slider. */
  label: string;
  /** How the current value reads, for the tooltip and `aria-valuetext`. */
  readout: string;
  /** What dragging does, appended to the tooltip. */
  hint: string;
  topGlyph: ReactNode;
  bottomGlyph: ReactNode;
}) {
  const trackRef = useRef<HTMLDivElement>(null);
  // True for the length of a drag. See the effect below.
  const draggingRef = useRef(false);

  /**
   * Stop a drag from selecting text across the rest of the page.
   *
   * The track sets `user-select: none`, which is enough to keep a selection
   * from *starting* on it — but `setPointerCapture` only retargets pointer
   * events. Native selection runs off the mouse events underneath, and those
   * still reach whatever the cursor happens to be over, so a drag that left
   * the track dragged a highlight across the panel behind it.
   *
   * Suppressing `selectstart` for the duration is the narrow fix. Preventing
   * the default on `pointerdown` would also work and is the usual advice,
   * but it suppresses the compatibility mouse events with it — including the
   * double-click that resets this slider.
   *
   * The listener lives for the component's lifetime and reads a ref, rather
   * than being added and removed around each drag. A listener added on
   * pointerdown has to be removed on an event that is not guaranteed to
   * arrive, and the failure mode there is a page where nothing can be
   * selected until reload. This way React's own teardown is the only
   * cleanup that has to work, and a missed pointerup costs nothing.
   */
  useEffect(() => {
    const suppress = (e: Event) => {
      if (draggingRef.current) e.preventDefault();
    };
    const end = () => {
      draggingRef.current = false;
    };
    document.addEventListener("selectstart", suppress);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    return () => {
      document.removeEventListener("selectstart", suppress);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
    };
  }, []);
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
      draggingRef.current = true;
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
        onChange(SLIDER_MAX);
      } else if (e.key === "End") {
        e.preventDefault();
        onChange(SLIDER_MIN);
      }
    },
    [onChange, value],
  );

  return (
    <div
      className="canvas-toolbar"
      // Horizontal padding matches `.canvas-toolbar`'s own 4px so this box and
      // the viewport-control box below come out the same width; the track picks
      // up `--tool-btn-size`, which is what sets that box's content width.
      style={{ flexDirection: "column", gap: 3, padding: "6px 4px" }}
    >
      {/* End glyphs carry the direction: spreading horizontally at the top,
          vertically at the bottom. A vertical slider whose "up" means "wider"
          needs saying, and an icon says it without a label. */}
      {topGlyph}
      <div
        ref={trackRef}
        role="slider"
        tabIndex={0}
        aria-label={label}
        aria-valuemin={SLIDER_MIN}
        aria-valuemax={SLIDER_MAX}
        aria-valuenow={Math.round(value)}
        aria-valuetext={readout}
        data-tooltip={`${label}: ${readout}. ${hint}${value === SLIDER_DEFAULT ? "" : " (double-click to reset)"}`}
        data-tooltip-pos="left"
        onPointerDown={handlePointerDown}
        onPointerMove={handlePointerMove}
        onDoubleClick={() => onChange(SLIDER_DEFAULT)}
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
      {bottomGlyph}
    </div>
  );
}
