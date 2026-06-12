import { XMarkIcon } from "@heroicons/react/24/outline";
import type { Task } from "../../../hooks";

export function formatSimClock(s: number): string {
  const total = Math.max(0, Math.round(s));
  const hh = Math.floor(total / 3600);
  const mm = Math.floor((total % 3600) / 60);
  const ss = total % 60;
  return `${hh}:${String(mm).padStart(2, "0")}:${String(ss).padStart(2, "0")}`;
}

/** Spinning or stopped ring icon used by running / queued tasks. */
export function RingIcon({
  percent,
  color,
}: {
  percent: number;
  color: string;
}) {
  const RING_R = 6;
  const RING_C = 2 * Math.PI * RING_R;
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" style={{ flexShrink: 0 }}>
      <title>Task progress</title>
      <circle
        cx="7"
        cy="7"
        r={RING_R}
        stroke="var(--border-hover)"
        strokeWidth="2"
        fill="none"
      />
      <circle
        cx="7"
        cy="7"
        r={RING_R}
        stroke={color}
        strokeWidth="2"
        fill="none"
        strokeDasharray={`${(percent / 100) * RING_C} ${RING_C}`}
        strokeLinecap="round"
        transform="rotate(-90 7 7)"
        style={{ transition: "stroke-dasharray 400ms ease" }}
      />
    </svg>
  );
}

export function PhaseBar({
  label,
  percent,
  done,
  queued,
  active,
  simulatedSeconds,
  durationSeconds,
}: {
  label: string;
  percent: number;
  done: boolean;
  queued: boolean;
  active: boolean;
  simulatedSeconds?: number;
  durationSeconds?: number;
}) {
  const barFill = done ? 100 : queued ? 0 : percent;
  const barColor = done ? "var(--status-success)" : "var(--accent)";
  const statusColor = done
    ? "var(--status-success)"
    : queued
      ? "var(--text-disabled)"
      : "var(--accent)";
  const statusText = done
    ? "Done"
    : queued
      ? "Waiting"
      : `${Math.round(percent)}%`;
  return (
    <div style={{ marginBottom: 5 }}>
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 3,
        }}
      >
        <span
          style={{
            fontSize: 9,
            fontWeight: 700,
            letterSpacing: "0.05em",
            textTransform: "uppercase",
            color: "var(--text-disabled)",
          }}
        >
          {label}
        </span>
        <span
          style={{
            fontSize: 9,
            fontVariantNumeric: "tabular-nums",
            color: statusColor,
            fontWeight: 600,
          }}
        >
          {statusText}
        </span>
      </div>
      <div
        style={{
          height: 3,
          background: "var(--border)",
          borderRadius: 2,
          overflow: "hidden",
        }}
      >
        <div
          style={{
            width: `${barFill}%`,
            height: "100%",
            background: queued ? "transparent" : barColor,
            borderRadius: 2,
            transition: "width 600ms ease",
          }}
        />
      </div>
      {active &&
        simulatedSeconds != null &&
        durationSeconds != null &&
        durationSeconds > 0 && (
          <div
            style={{
              fontSize: 10,
              color: "var(--text-disabled)",
              fontVariantNumeric: "tabular-nums",
              marginTop: 2,
            }}
          >
            {formatSimClock(simulatedSeconds)} /{" "}
            {formatSimClock(durationSeconds)}
          </div>
        )}
    </div>
  );
}

export function RunningCard({
  task,
  onCancel,
}: {
  task: Task;
  onCancel?: () => void;
}) {
  return (
    <div
      style={{
        padding: "12px 14px",
        borderBottom: "1px solid var(--border)",
        background: "var(--bg-card)",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: 8,
        }}
      >
        <RingIcon percent={task.progressPercent ?? 0} color="var(--accent)" />
        <div style={{ flex: 1, overflow: "hidden" }}>
          <div
            style={{
              fontSize: 13,
              fontWeight: 500,
              color: "var(--text-primary)",
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {task.projectName}
            <span style={{ color: "var(--text-tertiary)", fontWeight: 400 }}>
              {" "}
              /{" "}
            </span>
            {task.scenarioName}
          </div>
          <div style={{ fontSize: 11, color: "var(--accent)", marginTop: 1 }}>
            {task.progressMessage ?? "Solving…"}
          </div>
        </div>
        {onCancel && (
          <button
            type="button"
            onClick={onCancel}
            data-tooltip="Cancel run"
            style={{
              border: "none",
              background: "transparent",
              color: "var(--text-disabled)",
              cursor: "pointer",
              padding: 2,
              borderRadius: 3,
              flexShrink: 0,
              display: "flex",
              lineHeight: 0,
              transition: "color var(--t-fast)",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--status-error)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-disabled)";
            }}
          >
            <XMarkIcon style={{ width: 11, height: 11 }} />
          </button>
        )}
      </div>
      <PhaseBar
        label="Hydraulics"
        percent={task.hydraulicsPercent ?? 0}
        done={task.hydraulicsDone ?? false}
        queued={false}
        active={task.phase === "hydraulics"}
        simulatedSeconds={
          task.phase === "hydraulics" ? task.simulatedSeconds : undefined
        }
        durationSeconds={task.durationSeconds}
      />
      {task.hasQuality && (
        <PhaseBar
          label="Quality"
          percent={task.qualityPercent ?? 0}
          done={task.qualityDone ?? false}
          queued={task.phase === "hydraulics"}
          active={task.phase === "quality"}
          simulatedSeconds={
            task.phase === "quality" ? task.simulatedSeconds : undefined
          }
          durationSeconds={task.durationSeconds}
        />
      )}
    </div>
  );
}
