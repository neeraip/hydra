/* Bottom status bar — visible across the whole application.
   Shows: active project, solver state (idle / running / converged),
  iteration & residual readouts (live from SimulationProvider), issue counts. */

import {
  ExclamationCircleIcon,
  ExclamationTriangleIcon,
  InformationCircleIcon,
} from "@heroicons/react/24/outline";
import { useState } from "react";
import {
  useActiveProject,
  useAppState,
  useSimulation,
  useTasks,
} from "../../AppContext";
import { countIssues, LABEL, PILL } from "../../hooks";
import {
  formatShortcut,
  primaryModifierLabel,
  shiftModifierLabel,
} from "../../shortcuts";

type SolverState = "idle" | "loading" | "running" | "converged" | "warning";

export function StatusBar() {
  const { toggleIssuesPanel } = useAppState();
  const { project, accent } = useActiveProject();
  const { resultMeta, resultMetaLoading, issues } = useSimulation();
  const tasks = useTasks();

  // Derive solver state from simulation context.
  const hasRunning = tasks.some((t) => t.status === "running");
  const solver: SolverState = hasRunning
    ? "running"
    : resultMetaLoading
      ? "loading"
      : resultMeta
        ? "converged"
        : "idle";

  // Period count as a proxy for "work done".
  const timestepCount = resultMeta?.times.length ?? null;

  const solverColor = solverDotColor(solver);
  const solverBg = solverPillBg(solver);

  const [historyOpen, setHistoryOpen] = useState(false);
  const issueCounts = countIssues(issues);
  const issuesShortcut = formatShortcut([
    primaryModifierLabel(),
    shiftModifierLabel(),
    "M",
  ]);

  return (
    <div
      style={{
        height: 24,
        flexShrink: 0,
        background: "var(--bg-panel)",
        borderTop: "1px solid var(--border)",
        display: "flex",
        alignItems: "stretch",
        fontSize: 11,
        fontFamily: "var(--font-ui)",
        color: "var(--text-secondary)",
        position: "relative",
        zIndex: 25,
      }}
    >
      {/* Engine & project pill */}
      {project ? (
        <Pill
          background={`${accent}1f`}
          color={accent}
          title={`${LABEL} · ${project.name}`}
        >
          <span style={{ fontWeight: 600, letterSpacing: 0.4 }}>{PILL}</span>
          <span style={{ marginLeft: 6, color: "var(--text-secondary)" }}>
            {project.name}
          </span>
        </Pill>
      ) : (
        <Pill>
          <span style={{ color: "var(--text-tertiary)" }}>No project open</span>
        </Pill>
      )}

      {/* Issues counter — clickable to open IssuesPanel */}
      <button
        type="button"
        onClick={toggleIssuesPanel}
        disabled={!project}
        data-tooltip={`Issues & notifications (${issuesShortcut})`}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 8,
          background: "transparent",
          border: "none",
          borderLeft: "1px solid var(--border)",
          color: "var(--text-secondary)",
          padding: "0 10px",
          cursor: project ? "pointer" : "not-allowed",
          opacity: project ? undefined : 0.45,
          fontFamily: "var(--font-ui)",
          fontSize: 11,
        }}
      >
        <IconCount
          Icon={ExclamationCircleIcon}
          color="var(--status-error)"
          n={issueCounts.error}
        />
        <IconCount
          Icon={ExclamationTriangleIcon}
          color="var(--status-warning)"
          n={issueCounts.warn}
        />
        <IconCount
          Icon={InformationCircleIcon}
          color="#4a90d9"
          n={issueCounts.info}
        />
      </button>

      <div style={{ flex: 1 }} />

      {/* Solver state */}
      <Pill background={solverBg} color={solverColor}>
        <span
          aria-hidden
          style={{
            display: "inline-block",
            width: 6,
            height: 6,
            borderRadius: "50%",
            background: solverColor,
            animation:
              solver === "running"
                ? "pulse 1.4s ease-in-out infinite"
                : undefined,
          }}
        />
        <span style={{ marginLeft: 6, textTransform: "capitalize" }}>
          {solver}
        </span>
      </Pill>

      {/* Timestep count — shown after a simulation has run */}
      {project && timestepCount !== null && (
        <button
          type="button"
          onClick={() => setHistoryOpen((v) => !v)}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 10,
            background: historyOpen ? "var(--bg-card)" : "transparent",
            border: "none",
            color: "var(--text-secondary)",
            padding: "0 10px",
            cursor: "pointer",
            borderLeft: "1px solid var(--border)",
            fontFamily: "var(--font-mono)",
            fontSize: 11,
          }}
          data-tooltip="Simulation info"
        >
          <span>
            <span style={{ color: "var(--text-tertiary)" }}>steps</span>{" "}
            {timestepCount}
          </span>
        </button>
      )}

      {historyOpen && timestepCount !== null && (
        <SolverHistoryPopover
          onClose={() => setHistoryOpen(false)}
          timestepCount={timestepCount}
          accent={accent}
        />
      )}
    </div>
  );
}

function solverDotColor(s: SolverState): string {
  switch (s) {
    case "loading":
      return "#4a90d9";
    case "running":
      return "#d4a017";
    case "converged":
      return "#3daf75";
    case "warning":
      return "#c94040";
    default:
      return "var(--text-tertiary)";
  }
}
function solverPillBg(s: SolverState): string {
  switch (s) {
    case "loading":
      return "#4a90d922";
    case "running":
      return "#d4a01726";
    case "converged":
      return "#3daf7522";
    case "warning":
      return "#c9404022";
    default:
      return "transparent";
  }
}

function IconCount({
  Icon,
  color,
  n,
}: {
  Icon: React.ComponentType<{ style?: React.CSSProperties }>;
  color: string;
  n: number;
}) {
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 3,
        color: n > 0 ? color : "var(--text-disabled)",
        fontFamily: "var(--font-mono)",
        fontSize: 11,
      }}
    >
      <Icon style={{ width: 12, height: 12 }} />
      {n}
    </span>
  );
}

function Pill({
  children,
  background,
  color,
  title,
  mono,
}: {
  children: React.ReactNode;
  background?: string;
  color?: string;
  title?: string;
  mono?: boolean;
}) {
  return (
    <span
      data-tooltip={title}
      style={{
        display: "inline-flex",
        alignItems: "center",
        padding: "0 10px",
        borderLeft: "1px solid var(--border)",
        background: background ?? "transparent",
        color: color ?? "var(--text-secondary)",
        fontFamily: mono ? "var(--font-mono)" : undefined,
        whiteSpace: "nowrap",
      }}
    >
      {children}
    </span>
  );
}

function SolverHistoryPopover({
  onClose,
  timestepCount,
  accent,
}: {
  onClose: () => void;
  timestepCount: number;
  accent: string;
}) {
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: popover container only stops backdrop clicks.
    // biome-ignore lint/a11y/useKeyWithClickEvents: popover container only stops backdrop clicks.
    <div
      onClick={(e) => e.stopPropagation()}
      style={{
        position: "absolute",
        right: 220,
        bottom: 28,
        width: 220,
        padding: 12,
        background: "var(--bg-panel)",
        border: "1px solid var(--border)",
        borderRadius: 6,
        boxShadow: "var(--shadow-2)",
      }}
    >
      <div
        style={{
          fontSize: 10,
          fontWeight: 600,
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: 0.4,
          marginBottom: 8,
        }}
      >
        Last simulation
      </div>
      <div
        style={{
          fontSize: 12,
          color: "var(--text-primary)",
          fontFamily: "var(--font-mono)",
        }}
      >
        {timestepCount} timestep{timestepCount !== 1 ? "s" : ""} computed
      </div>
      <div
        style={{ fontSize: 11, color: "var(--text-tertiary)", marginTop: 4 }}
      >
        Detailed solver diagnostics will be available in a future update.
      </div>
      <button
        type="button"
        onClick={onClose}
        style={{
          marginTop: 10,
          fontSize: 11,
          color: accent,
          background: "transparent",
          border: "none",
          cursor: "pointer",
          fontFamily: "var(--font-ui)",
          padding: 0,
        }}
      >
        Dismiss
      </button>
    </div>
  );
}
