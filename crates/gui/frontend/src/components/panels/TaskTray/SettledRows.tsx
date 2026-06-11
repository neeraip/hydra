import { CheckIcon } from "@heroicons/react/16/solid";
import {
  ChevronDownIcon,
  ChevronRightIcon,
  XMarkIcon,
} from "@heroicons/react/24/outline";
import { useState } from "react";
import type { Task } from "../../../hooks";
import { PhaseBar } from "./RunningCard";

export function formatEventTime(ms: number): string {
  return new Date(ms).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function QueuedRow({
  task,
  position,
  onCancel,
}: {
  task: Task;
  position: number;
  onCancel: () => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "7px 14px",
        borderBottom: "1px solid var(--border)",
      }}
    >
      {/* Queue position badge */}
      <span
        style={{
          fontSize: 9,
          fontWeight: 700,
          color: "var(--text-disabled)",
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
          borderRadius: 3,
          padding: "1px 4px",
          flexShrink: 0,
          fontVariantNumeric: "tabular-nums",
        }}
      >
        #{position}
      </span>
      <div style={{ flex: 1, overflow: "hidden" }}>
        <div
          style={{
            fontSize: 12,
            color: "var(--text-secondary)",
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
          }}
        >
          {task.projectName}
          <span style={{ color: "var(--text-disabled)" }}> / </span>
          {task.scenarioName}
        </div>
      </div>
      <button
        onClick={onCancel}
        data-tooltip="Remove from queue"
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
    </div>
  );
}

export function CompletedRow({
  task,
  onViewResults,
  onDismiss,
}: {
  task: Task;
  onViewResults: () => void;
  onDismiss: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const historyEntries = (task.history ?? []).slice(-6).reverse();
  const Chevron = expanded ? ChevronDownIcon : ChevronRightIcon;

  return (
    <div style={{ borderBottom: "1px solid var(--border)" }}>
      {/* Collapsed one-line row */}
      <div
        onClick={() => setExpanded((x) => !x)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "7px 14px",
          cursor: "pointer",
        }}
        onMouseEnter={(e) => {
          (e.currentTarget as HTMLDivElement).style.background =
            "var(--nav-hover)";
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLDivElement).style.background = "";
        }}
      >
        <CheckIcon
          style={{
            width: 12,
            height: 12,
            color: "var(--status-success)",
            flexShrink: 0,
          }}
        />
        <div style={{ flex: 1, overflow: "hidden" }}>
          <div
            style={{
              fontSize: 12,
              color: "var(--text-secondary)",
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {task.projectName}
            <span style={{ color: "var(--text-disabled)" }}> / </span>
            {task.scenarioName}
          </div>
        </div>
        {/* "View results" link — click without expanding */}
        {task.primaryAction === "View results" && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onViewResults();
            }}
            style={{
              border: "none",
              background: "none",
              padding: 0,
              color: "var(--accent)",
              fontSize: 11,
              cursor: "pointer",
              fontFamily: "var(--font-ui)",
              flexShrink: 0,
              transition: "color var(--t-fast)",
            }}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--text-primary)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.color =
                "var(--accent)";
            }}
          >
            View results
          </button>
        )}
        <button
          onClick={(e) => {
            e.stopPropagation();
            onDismiss();
          }}
          data-tooltip="Dismiss"
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
              "var(--text-secondary)";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLButtonElement).style.color =
              "var(--text-disabled)";
          }}
        >
          <XMarkIcon style={{ width: 11, height: 11 }} />
        </button>
        <Chevron
          style={{
            width: 10,
            height: 10,
            color: "var(--text-disabled)",
            flexShrink: 0,
          }}
        />
      </div>

      {/* Expanded detail */}
      {expanded && (
        <div
          style={{ padding: "0 14px 10px 34px", background: "var(--bg-card)" }}
        >
          <div style={{ marginBottom: 8 }}>
            <PhaseBar
              label="Hydraulics"
              percent={100}
              done={true}
              queued={false}
              active={false}
            />
            {task.hasQuality && (
              <PhaseBar
                label="Quality"
                percent={100}
                done={true}
                queued={false}
                active={false}
              />
            )}
          </div>
          <div style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
            {task.timeLabel}
          </div>
          {historyEntries.length > 0 && (
            <div style={{ marginTop: 6, display: "grid", gap: 2 }}>
              {historyEntries.map((entry, i) => (
                <div
                  key={`${entry.at}-${i}`}
                  style={{
                    display: "grid",
                    gridTemplateColumns: "68px 1fr",
                    gap: 6,
                    fontSize: 10,
                    color: "var(--text-disabled)",
                  }}
                >
                  <span style={{ fontVariantNumeric: "tabular-nums" }}>
                    {formatEventTime(entry.at)}
                  </span>
                  <span
                    style={{
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {entry.label}
                  </span>
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export function FailedRow({
  task,
  onDismiss,
}: {
  task: Task;
  onDismiss: () => void;
}) {
  return (
    <div
      style={{ padding: "8px 14px", borderBottom: "1px solid var(--border)" }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          marginBottom: task.errorMessage ? 5 : 0,
        }}
      >
        <XMarkIcon
          style={{
            width: 12,
            height: 12,
            color: "var(--status-error)",
            flexShrink: 0,
          }}
        />
        <div style={{ flex: 1, overflow: "hidden" }}>
          <div
            style={{
              fontSize: 12,
              color: "var(--text-secondary)",
              whiteSpace: "nowrap",
              overflow: "hidden",
              textOverflow: "ellipsis",
            }}
          >
            {task.projectName}
            <span style={{ color: "var(--text-disabled)" }}> / </span>
            {task.scenarioName}
          </div>
        </div>
        <button
          onClick={onDismiss}
          data-tooltip="Dismiss"
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
              "var(--text-secondary)";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLButtonElement).style.color =
              "var(--text-disabled)";
          }}
        >
          <XMarkIcon style={{ width: 11, height: 11 }} />
        </button>
      </div>
      {task.errorMessage && (
        <div
          style={{
            fontSize: 11,
            color: "var(--status-error)",
            background: "rgba(201, 64, 64, 0.08)",
            border: "1px solid rgba(201, 64, 64, 0.18)",
            borderRadius: 4,
            padding: "4px 7px",
            lineHeight: 1.45,
          }}
        >
          {task.errorMessage}
        </div>
      )}
    </div>
  );
}
