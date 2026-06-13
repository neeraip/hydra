import {
  BackwardIcon,
  ChevronDoubleLeftIcon,
  ChevronDoubleRightIcon,
  ForwardIcon,
  PauseIcon,
  PlayIcon,
} from "@heroicons/react/16/solid";
import { useCallback, useMemo, useRef } from "react";
import type { ResultMeta } from "../hooks";

/** Fallback time-axis labels (00:00–24:00) used before resultMeta is loaded. */
const EMPTY_LABELS: string[] = Array.from(
  { length: 25 },
  (_, i) => `${String(i).padStart(2, "0")}:00`,
);

/** Height of the scrubber track rail in pixels. */
const TRACK_H = 8;

export function Timeline({
  currentHour,
  setCurrentHour,
  isPlaying,
  setIsPlaying,
  speed,
  setSpeed,
  loop,
  setLoop,
  hoverHour,
  setHoverHour,
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
  hoverHour: number | null;
  setHoverHour: (h: number | null) => void;
  resultMeta?: ResultMeta | null;
  maxStep?: number;
  steadyState?: boolean;
}) {
  const effectiveMaxStep = maxStep ?? 24;

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

  const trackRef = useRef<HTMLDivElement | null>(null);

  const step = useCallback(
    (delta: number) => {
      const next = Math.max(0, Math.min(effectiveMaxStep, currentHour + delta));
      setCurrentHour(next);
    },
    [effectiveMaxStep, currentHour, setCurrentHour],
  );

  const onTrackClick = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const r = trackRef.current?.getBoundingClientRect();
      if (!r) return;
      const frac = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width));
      setCurrentHour(Math.round(frac * effectiveMaxStep));
    },
    [effectiveMaxStep, setCurrentHour],
  );

  const onTrackMove = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const r = trackRef.current?.getBoundingClientRect();
      if (!r) return;
      const frac = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width));
      setHoverHour(Math.round(frac * effectiveMaxStep));
    },
    [effectiveMaxStep, setHoverHour],
  );

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
        <button
          type="button"
          className="tl-btn"
          onClick={() => setCurrentHour(0)}
          data-tooltip="Jump to start (Home)"
          style={{
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <BackwardIcon style={{ width: 14, height: 14 }} />
        </button>
        <button
          type="button"
          className="tl-btn"
          onClick={() => step(-1)}
          data-tooltip="Step back (←)"
          style={{
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <ChevronDoubleLeftIcon style={{ width: 14, height: 14 }} />
        </button>
        <button
          type="button"
          className="tl-btn tl-play"
          onClick={() => setIsPlaying(!isPlaying)}
          data-tooltip={isPlaying ? "Pause (Space)" : "Play (Space)"}
          style={{
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          {isPlaying ? (
            <PauseIcon style={{ width: 14, height: 14 }} />
          ) : (
            <PlayIcon style={{ width: 14, height: 14 }} />
          )}
        </button>
        <button
          type="button"
          className="tl-btn"
          onClick={() => step(1)}
          data-tooltip="Step forward (→)"
          style={{
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <ChevronDoubleRightIcon style={{ width: 14, height: 14 }} />
        </button>
        <button
          type="button"
          className="tl-btn"
          onClick={() => setCurrentHour(effectiveMaxStep)}
          data-tooltip="Jump to end (End)"
          style={{
            display: "inline-flex",
            alignItems: "center",
            justifyContent: "center",
          }}
        >
          <ForwardIcon style={{ width: 14, height: 14 }} />
        </button>
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
          {timeLabels[currentHour] ?? timeLabels[timeLabels.length - 1]}
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
            const idx = Math.round(frac * effectiveMaxStep);
            const label = timeLabels[idx] ?? timeLabels[timeLabels.length - 1];
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
