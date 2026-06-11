import type React from "react";
import type { SimParams } from "../../../hooks";

// ── Read-only summary ────────────────────────────────────────────────────────

export function SummaryGrid({ params }: { params: SimParams }) {
  const headloss =
    params.headLossFormula === "H-W"
      ? "Hazen–Williams"
      : params.headLossFormula === "D-W"
        ? "Darcy–Weisbach"
        : "Chézy–Manning";
  const demandModel =
    params.demandModel === "DDA" ? "Demand-driven" : "Pressure-driven";
  const quality =
    params.qualityMode === "none"
      ? "None"
      : params.qualityMode === "chemical"
        ? `Chemical${params.chemName ? ` (${params.chemName})` : ""}`
        : params.qualityMode === "age"
          ? "Water age"
          : `Source trace${params.traceNode ? ` (${params.traceNode})` : ""}`;

  const rows: Array<{ label: string; value: string }> = [
    { label: "Duration", value: fmtHours(params.duration) },
    { label: "Start clock", value: fmtClock(params.startClocktime) },
    { label: "Hydraulic step", value: fmtMinutes(params.hydStep) },
    { label: "Pattern step", value: fmtMinutes(params.patternStep) },
    { label: "Report step", value: fmtMinutes(params.reportStep) },
    { label: "Headloss", value: headloss },
    { label: "Demand model", value: demandModel },
    {
      label: "Demand multiplier",
      value: stripTrailingZeros(params.demandMultiplier),
    },
    { label: "Quality", value: quality },
  ];
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(2, minmax(0, 1fr))",
        gap: "6px 16px",
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius: 6,
        padding: "10px 12px",
      }}
    >
      {rows.map((r) => (
        <div
          key={r.label}
          style={{
            display: "flex",
            justifyContent: "space-between",
            gap: 8,
            minWidth: 0,
          }}
        >
          <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
            {r.label}
          </span>
          <span
            style={{
              fontSize: 12,
              color: "var(--text-primary)",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              minWidth: 0,
            }}
          >
            {r.value}
          </span>
        </div>
      ))}
    </div>
  );
}

export function Label({ children }: { children: React.ReactNode }) {
  return (
    <span
      style={{
        fontSize: 11,
        color: "var(--text-tertiary)",
        textTransform: "uppercase",
        letterSpacing: "0.05em",
        fontWeight: 600,
      }}
    >
      {children}
    </span>
  );
}

export const SIM_STATE_META: Record<string, { label: string; color: string }> =
  {
    simulated: { label: "Simulated", color: "var(--status-success, #22c55e)" },
    running: { label: "Running…", color: "var(--accent, #4a90d9)" },
    queued: { label: "Queued", color: "var(--text-tertiary)" },
    stale: { label: "Edited", color: "#f59e0b" },
    failed: { label: "Failed", color: "var(--status-error, #ef4444)" },
    "not-run": { label: "Not run", color: "var(--text-tertiary)" },
  };

export function SimStateBadge({ state }: { state: string }) {
  const meta = SIM_STATE_META[state] ?? {
    label: state,
    color: "var(--text-tertiary)",
  };
  return (
    <span
      style={{
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: "0.04em",
        color: meta.color,
        padding: "1px 0",
        flexShrink: 0,
        fontFamily: "var(--font-ui)",
      }}
    >
      {meta.label}
    </span>
  );
}

export function ActiveBadge() {
  return (
    <span
      style={{
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: "0.04em",
        color: "var(--accent)",
        background: "rgba(100,160,255,0.12)",
        border: "1px solid rgba(100,160,255,0.25)",
        padding: "1px 6px",
        borderRadius: 3,
      }}
    >
      active
    </span>
  );
}

export function fmtHours(seconds: number): string {
  if (seconds <= 0) return "0 h";
  const h = seconds / 3600;
  if (Number.isInteger(h)) return `${h} h`;
  return `${h.toFixed(2).replace(/\.?0+$/, "")} h`;
}

export function fmtMinutes(seconds: number): string {
  if (seconds <= 0) return "0 min";
  if (seconds % 3600 === 0) return `${seconds / 3600} h`;
  if (seconds % 60 === 0) return `${seconds / 60} min`;
  return `${seconds} s`;
}

export function fmtClock(seconds: number): string {
  const h = Math.floor(seconds / 3600) % 24;
  const m = Math.floor((seconds % 3600) / 60);
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

export function stripTrailingZeros(n: number): string {
  return n.toFixed(3).replace(/\.?0+$/, "");
}
