import {
  BackwardIcon,
  ChevronDoubleLeftIcon,
  ChevronDoubleRightIcon,
  ForwardIcon,
  PauseIcon,
  PlayIcon,
} from "@heroicons/react/16/solid";
import {
  type CSSProperties,
  type ReactNode,
  useCallback,
  useMemo,
  useRef,
  useState,
} from "react";
import type { ResultMeta } from "../hooks";

/** Fallback time-axis labels (00:00–24:00) used before resultMeta is loaded. */
const EMPTY_LABELS: string[] = Array.from(
  { length: 25 },
  (_, i) => `${String(i).padStart(2, "0")}:00`,
);

/** Height of the scrubber track rail in pixels. */
const TRACK_H = 8;

const TRANSPORT_BTN_STYLE: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
};

const TRANSPORT_ICON_STYLE: CSSProperties = { width: 14, height: 14 };

/** Icon button in the transport cluster — same DOM as a plain `.tl-btn`. */
function TransportButton({
  className = "tl-btn",
  onClick,
  tooltip,
  children,
}: {
  className?: string;
  onClick: () => void;
  tooltip: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className={className}
      onClick={onClick}
      data-tooltip={tooltip}
      style={TRANSPORT_BTN_STYLE}
    >
      {children}
    </button>
  );
}

export function Timeline({
  currentHour,
  setCurrentHour,
  isPlaying,
  setIsPlaying,
  speed,
  setSpeed,
  loop,
  setLoop,
  resultMeta,
  maxStep,
  steadyState,
}: {
  currentHour: number;
  setCurrentHour: (h: number) => void;
  isPlaying: boolean;
  setIsPlaying: (v: boolean) => void;
  speed: number;
  setSpeed: (v: number) => void;
  loop: boolean;
  setLoop: (v: boolean) => void;
  resultMeta?: ResultMeta | null;
  maxStep?: number;
  steadyState?: boolean;
}) {
  // Local: only this component reads the hover marker, and lifting it to
  // CanvasView made every scrubber mousemove re-render the whole canvas view.
  const [hoverHour, setHoverHour] = useState<number | null>(null);
  // Floor at 1 for percentage math: a single-period result has maxStep 0 and
  // dividing by it produced NaN% marker offsets.
  const effectiveMaxStep = Math.max(1, maxStep ?? 24);

  // Time labels ("HH:MM") derived from resultMeta snapshot times.
  const liveLabels = useMemo(() => {
    if (!resultMeta || resultMeta.times.length === 0) return null;
    return resultMeta.times.map((t) => {
      const h = Math.floor(t / 3600);
      const m = Math.floor((t % 3600) / 60);
      return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
    });
  }, [resultMeta]);

  const timeLabels = liveLabels ?? EMPTY_LABELS;

  /** Label for a step index; clamps out-of-range indices to the last label. */
  const labelAt = (i: number) =>
    timeLabels[i] ?? timeLabels[timeLabels.length - 1];

  const trackRef = useRef<HTMLDivElement | null>(null);

  const step = useCallback(
    (delta: number) => {
      const next = Math.max(0, Math.min(effectiveMaxStep, currentHour + delta));
      setCurrentHour(next);
    },
    [effectiveMaxStep, currentHour, setCurrentHour],
  );

  /** Clamped 0–1 fraction along the track for a mouse position, or null when
   * the track isn't measurable yet. */
  const trackFrac = useCallback((clientX: number): number | null => {
    const r = trackRef.current?.getBoundingClientRect();
    if (!r) return null;
    return Math.max(0, Math.min(1, (clientX - r.left) / r.width));
  }, []);

  const onTrackClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const frac = trackFrac(e.clientX);
      if (frac == null) return;
      setCurrentHour(Math.round(frac * effectiveMaxStep));
    },
    [effectiveMaxStep, setCurrentHour, trackFrac],
  );

  const onTrackMove = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const frac = trackFrac(e.clientX);
      if (frac == null) return;
      setHoverHour(Math.round(frac * effectiveMaxStep));
    },
    [effectiveMaxStep, trackFrac],
  );

  // ── Drag-scrubbing ────────────────────────────────────────────────────────
  // pointerdown on the track starts a scrub (pointer capture keeps move/up
  // events flowing even when the cursor leaves the track), pointermove
  // updates the playhead live, pointerup ends the scrub. Plain clicks still
  // work via onClick (it fires after pointerup with the same value — a
  // harmless no-op set), and keyboard handling is untouched.
  const scrubbingRef = useRef(false);
  const scrubTo = useCallback(
    (clientX: number) => {
      const frac = trackFrac(clientX);
      if (frac == null) return;
      setCurrentHour(Math.round(frac * effectiveMaxStep));
    },
    [effectiveMaxStep, setCurrentHour, trackFrac],
  );
  const onTrackPointerDown = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      // Primary button / touch / pen only — don't scrub on right-click.
      if (e.pointerType === "mouse" && e.button !== 0) return;
      scrubbingRef.current = true;
      e.currentTarget.setPointerCapture(e.pointerId);
      scrubTo(e.clientX);
    },
    [scrubTo],
  );
  const onTrackPointerMove = useCallback(
    (e: React.PointerEvent<HTMLDivElement>) => {
      if (!scrubbingRef.current) return;
      scrubTo(e.clientX);
      setHoverHour(null);
    },
    [scrubTo],
  );
  const endScrub = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (!scrubbingRef.current) return;
    scrubbingRef.current = false;
    if (e.currentTarget.hasPointerCapture(e.pointerId)) {
      e.currentTarget.releasePointerCapture(e.pointerId);
    }
  }, []);

  const playheadFrac =
    effectiveMaxStep > 0 ? currentHour / effectiveMaxStep : 0;
  const handleTrackKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        step(-1);
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        step(1);
      } else if (e.key === "Home") {
        e.preventDefault();
        setCurrentHour(0);
      } else if (e.key === "End") {
        e.preventDefault();
        setCurrentHour(effectiveMaxStep);
      }
    },
    [effectiveMaxStep, setCurrentHour, step],
  );

  // Tick mark positions: up to 5 ticks evenly spaced across the step range.
  const tickFracs = useMemo(() => {
    const count = Math.min(5, effectiveMaxStep + 1);
    if (count <= 1) return [0];
    return Array.from({ length: count }, (_, i) => i / (count - 1));
  }, [effectiveMaxStep]);

  if (steadyState) {
    const snapshotLabel = timeLabels[0] ?? "00:00";
    return (
      <div className="timeline-bar timeline-bar--steady">
        <span className="timeline-steady-pill">Steady-state</span>
        <span className="timeline-steady-text">
          Single hydraulic snapshot at {snapshotLabel}
        </span>
      </div>
    );
  }

  return (
    <div className="timeline-bar">
      {/* Transport controls */}
      <div
        style={{ display: "flex", alignItems: "center", gap: 4, flexShrink: 0 }}
      >
        <TransportButton
          onClick={() => setCurrentHour(0)}
          tooltip="Jump to start (Home)"
        >
          <BackwardIcon style={TRANSPORT_ICON_STYLE} />
        </TransportButton>
        <TransportButton onClick={() => step(-1)} tooltip="Step back (←)">
          <ChevronDoubleLeftIcon style={TRANSPORT_ICON_STYLE} />
        </TransportButton>
        <TransportButton
          className="tl-btn tl-play"
          onClick={() => setIsPlaying(!isPlaying)}
          tooltip={isPlaying ? "Pause (Space)" : "Play (Space)"}
        >
          {isPlaying ? (
            <PauseIcon style={TRANSPORT_ICON_STYLE} />
          ) : (
            <PlayIcon style={TRANSPORT_ICON_STYLE} />
          )}
        </TransportButton>
        <TransportButton onClick={() => step(1)} tooltip="Step forward (→)">
          <ChevronDoubleRightIcon style={TRANSPORT_ICON_STYLE} />
        </TransportButton>
        <TransportButton
          onClick={() => setCurrentHour(effectiveMaxStep)}
          tooltip="Jump to end (End)"
        >
          <ForwardIcon style={TRANSPORT_ICON_STYLE} />
        </TransportButton>
      </div>

      {/* Speed + loop */}
      <div
        style={{ display: "flex", alignItems: "center", gap: 4, flexShrink: 0 }}
      >
        <select
          className="tl-speed"
          value={speed}
          onChange={(e) => setSpeed(Number(e.target.value))}
          data-tooltip="Playback speed"
        >
          <option value={0.5}>0.5×</option>
          <option value={1}>1×</option>
          <option value={2}>2×</option>
          <option value={4}>4×</option>
          <option value={8}>8×</option>
        </select>
        <button
          type="button"
          className={`tl-btn ${loop ? "tl-active" : ""}`}
          onClick={() => setLoop(!loop)}
          data-tooltip={loop ? "Loop on" : "Loop off"}
        >
          ⟳
        </button>
      </div>

      {/* Time readout */}
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "flex-start",
          flexShrink: 0,
          gap: 1,
        }}
      >
        <span
          style={{
            fontSize: 17,
            fontFamily: "var(--font-mono)",
            color: "var(--text-primary)",
            fontWeight: 600,
            lineHeight: 1,
          }}
        >
          {labelAt(currentHour)}
        </span>
        <span
          style={{
            fontSize: 10,
            color: "var(--text-tertiary)",
            fontFamily: "var(--font-mono)",
          }}
        >
          step {currentHour.toString().padStart(2, "0")} / {effectiveMaxStep}
        </span>
      </div>

      {/* Scrubber track */}
      <div
        ref={trackRef}
        role="slider"
        aria-label="Simulation timeline"
        aria-valuemin={0}
        aria-valuemax={effectiveMaxStep}
        aria-valuenow={currentHour}
        tabIndex={0}
        onClick={onTrackClick}
        onKeyDown={handleTrackKeyDown}
        onMouseMove={onTrackMove}
        onMouseLeave={() => setHoverHour(null)}
        onPointerDown={onTrackPointerDown}
        onPointerMove={onTrackPointerMove}
        onPointerUp={endScrub}
        onPointerCancel={endScrub}
        style={{
          flex: 1,
          height: TRACK_H + 16,
          position: "relative",
          cursor: "pointer",
          display: "flex",
          flexDirection: "column",
          justifyContent: "center",
        }}
      >
        {/* Rail */}
        <div
          style={{
            position: "relative",
            height: TRACK_H,
            borderRadius: TRACK_H / 2,
            background: "rgba(0,0,0,0.35)",
            border: "1px solid var(--border)",
            overflow: "visible",
          }}
        >
          {/* Fill */}
          <div
            style={{
              position: "absolute",
              left: 0,
              top: 0,
              bottom: 0,
              width: `${playheadFrac * 100}%`,
              borderRadius: TRACK_H / 2,
              background: "var(--accent)",
              opacity: 0.55,
              transition: "width 80ms linear",
            }}
          />

          {/* Tick marks */}
          {tickFracs.map((frac) => (
            <div
              key={`tick-${frac}`}
              style={{
                position: "absolute",
                left: `${frac * 100}%`,
                top: 0,
                bottom: 0,
                width: 1,
                background: "rgba(255,255,255,0.08)",
                pointerEvents: "none",
              }}
            />
          ))}

          {/* Hover indicator */}
          {hoverHour !== null && hoverHour !== currentHour && (
            <div
              style={{
                position: "absolute",
                left: `${(hoverHour / effectiveMaxStep) * 100}%`,
                top: -2,
                bottom: -2,
                width: 1,
                background: "rgba(255,255,255,0.3)",
                pointerEvents: "none",
              }}
            />
          )}

          {/* Playhead handle */}
          <div
            style={{
              position: "absolute",
              left: `calc(${playheadFrac * 100}% - 7px)`,
              top: "50%",
              transform: "translateY(-50%)",
              width: 14,
              height: 14,
              borderRadius: "50%",
              background: "var(--accent)",
              border: "2px solid var(--bg-panel)",
              boxShadow: "0 0 0 1px var(--accent)",
              pointerEvents: "none",
            }}
          />
        </div>

        {/* Time-axis labels */}
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            marginTop: 4,
            fontSize: 9,
            color: "var(--text-tertiary)",
            fontFamily: "var(--font-mono)",
            pointerEvents: "none",
          }}
        >
          {tickFracs.map((frac) => {
            const label = labelAt(Math.round(frac * effectiveMaxStep));
            return <span key={`${label}-${frac}`}>{label}</span>;
          })}
        </div>

        {/* Hover tooltip */}
        {hoverHour !== null && (
          <div
            style={{
              position: "absolute",
              left: `${(hoverHour / effectiveMaxStep) * 100}%`,
              top: -4,
              transform: "translate(-50%, -100%)",
              background: "rgba(20,22,26,0.96)",
              border: "1px solid var(--border)",
              borderRadius: 4,
              padding: "3px 6px",
              fontSize: 10,
              fontFamily: "var(--font-mono)",
              color: "var(--text-primary)",
              pointerEvents: "none",
              whiteSpace: "nowrap",
              boxShadow: "var(--shadow-2)",
            }}
          >
            {timeLabels[hoverHour] ?? ""}
          </div>
        )}
      </div>
    </div>
  );
}
