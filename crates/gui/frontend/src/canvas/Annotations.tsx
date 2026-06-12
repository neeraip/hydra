import { ArrowsRightLeftIcon } from "@heroicons/react/16/solid";
import type React from "react";
import { formatMeters, pixelDistance } from "./coords";
import type { CanvasTool, ClickPoint, ViewMode } from "./types";

export function MeasureOverlay({ points }: { points: ClickPoint[] }) {
  if (points.length === 0) return null;
  const a = points[0];
  const b = points[1];

  return (
    <g pointerEvents="none">
      {/* First point marker */}
      <circle
        cx={a.x}
        cy={a.y}
        r={4}
        fill="#d4a017"
        stroke="rgba(0,0,0,0.6)"
        strokeWidth={1}
      />
      {b && (
        <>
          {/* Connecting line */}
          <line
            x1={a.x}
            y1={a.y}
            x2={b.x}
            y2={b.y}
            stroke="#d4a017"
            strokeWidth={1.5}
            strokeDasharray="6 3"
          />
          {/* End marker */}
          <circle
            cx={b.x}
            cy={b.y}
            r={4}
            fill="#d4a017"
            stroke="rgba(0,0,0,0.6)"
            strokeWidth={1}
          />
          {/* Distance label */}
          <DistanceLabel ax={a.x} ay={a.y} bx={b.x} by={b.y} />
        </>
      )}
    </g>
  );
}

function DistanceLabel({
  ax,
  ay,
  bx,
  by,
}: {
  ax: number;
  ay: number;
  bx: number;
  by: number;
}) {
  const mx = (ax + bx) / 2;
  const my = (ay + by) / 2;
  const label = formatMeters(pixelDistance(ax, ay, bx, by));
  // Approximate label width — keeps the chip from overflowing for short text.
  const w = Math.max(label.length * 7 + 14, 60);
  return (
    <g>
      <rect
        x={mx - w / 2}
        y={my - 22}
        width={w}
        height={18}
        rx={4}
        fill="rgba(20,22,26,0.92)"
        stroke="rgba(212,160,23,0.5)"
        strokeWidth={1}
      />
      <text
        x={mx}
        y={my - 9}
        textAnchor="middle"
        fontSize={11}
        fill="#d4a017"
        style={{ fontFamily: "var(--font-mono)", fontWeight: 600 }}
      >
        {label}
      </text>
    </g>
  );
}

export function AnnotationSummary({
  tool,
  measurePts,
  measureGeoPts,
  measureDistanceM,
  viewMode,
  onClear,
}: {
  tool: CanvasTool;
  measurePts: ClickPoint[];
  measureGeoPts?: { lng: number; lat: number }[];
  measureDistanceM?: number | null;
  viewMode?: ViewMode;
  onClear: () => void;
}) {
  const measuring = tool === "measure";
  const isMap = viewMode === "map";

  // Determine whether either set of points has a completed measurement.
  const hasResult = isMap
    ? (measureGeoPts?.length ?? 0) >= 2
    : measurePts.length >= 2;

  let body: React.ReactNode;
  if (
    measuring &&
    !hasResult &&
    (measureGeoPts?.length ?? 0) === 0 &&
    measurePts.length === 0
  ) {
    body = (
      <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
        Click two points on the canvas to measure.
      </span>
    );
  } else if (measuring && !hasResult) {
    body = (
      <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
        Click a second point…
      </span>
    );
  } else if (hasResult) {
    const distM = isMap
      ? (measureDistanceM ?? 0)
      : (() => {
          const [a, b] = measurePts;
          return pixelDistance(a.x, a.y, b.x, b.y);
        })();
    body = (
      <span style={{ fontSize: 12, color: "var(--text-secondary)" }}>
        Distance:{" "}
        <span
          style={{
            color: "#d4a017",
            fontFamily: "var(--font-mono)",
            fontWeight: 600,
          }}
        >
          {formatMeters(distM)}
        </span>
      </span>
    );
  } else {
    return null;
  }

  return (
    <div
      style={{
        position: "absolute",
        bottom: 12,
        left: 12,
        zIndex: 10,
        background: "var(--bg-panel)",
        border: `1px solid rgba(212,160,23,0.4)`,
        borderRadius: 8,
        padding: "8px 12px",
        display: "flex",
        alignItems: "center",
        gap: 12,
        boxShadow: "var(--shadow-2)",
      }}
    >
      <span
        style={{
          fontSize: 10,
          fontWeight: 700,
          letterSpacing: "0.06em",
          color: "#d4a017",
          textTransform: "uppercase",
        }}
      >
        <ArrowsRightLeftIcon
          style={{ width: 14, height: 14, verticalAlign: "middle" }}
        />{" "}
        Measure
      </span>
      {body}
      {hasResult && (
        <button
          type="button"
          onClick={onClear}
          style={{
            background: "transparent",
            border: "1px solid var(--border)",
            color: "var(--text-tertiary)",
            borderRadius: 5,
            padding: "3px 8px",
            fontSize: 11,
            cursor: "pointer",
            fontFamily: "var(--font-ui)",
          }}
        >
          Clear
        </button>
      )}
    </div>
  );
}
