import {
  flowColor,
  pressureColor,
  qualityColor,
  sequentialColor,
  statusColor,
  velocityColor,
} from "../../../canvas/colors";
import type { LinkVariable, NodeVariable } from "../../../canvas/types";
import type { Link, Node, ResultRanges } from "../../../hooks";
import { formatQty, useUnitSystem } from "../../../units";
import { BigValue, SecondaryCell } from "./primitives";

// ── Empty state (no simulation run yet) ─────────────────────────────────────

function EmptyStateCard() {
  return (
    <div
      style={{
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius: 8,
        padding: "16px 12px",
        marginBottom: 14,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <span
        style={{
          fontSize: 12,
          color: "var(--text-secondary)",
          fontFamily: "var(--font-ui)",
        }}
      >
        Run a simulation to see results
      </span>
    </div>
  );
}

export const LINK_TYPE_COLOR: Record<string, string> = {
  pipe: "var(--text-secondary)",
  pump: "#d4a017",
  valve: "#f97316",
};

/**
 * Human label for Hydra OUT-file status codes (status_to_f32):
 * 0=XHead, 1=TempClosed, 2=Closed, 3=Open, 4=Active, 6=XFcv, 7=XPressure
 */
export function statusLabel(s: number | null | undefined): string {
  if (s === 3) return "Open";
  if (s === 2) return "Closed";
  if (s === 4) return "Active";
  if (s === 0) return "Closed (XHead)";
  if (s === 1) return "Temp Closed";
  if (s === 6) return "Active (XFcv)";
  if (s === 7) return "Active (XPressure)";
  return "—";
}

// ── Results cards ─────────────────────────────────────────────────────────────

export function NodeResultsCard({
  node,
  accent,
  nodeVar,
  ranges,
  hasSimulation,
}: {
  node: Node;
  accent: string;
  nodeVar?: NodeVariable;
  ranges?: ResultRanges;
  hasSimulation?: boolean;
}) {
  const sys = useUnitSystem();
  const hasSim =
    node.pressure != null ||
    node.demand != null ||
    node.head != null ||
    node.quality != null;
  if (!hasSim && !hasSimulation) return <EmptyStateCard />;

  function valueColor(variable: NodeVariable, value: number): string {
    if (!nodeVar || nodeVar !== variable) return accent;
    switch (variable) {
      case "pressure":
        return pressureColor(value);
      case "head":
        return ranges
          ? sequentialColor(value, ranges.headMin, ranges.headMax)
          : accent;
      case "demand":
        return ranges
          ? sequentialColor(value, ranges.demandMin, ranges.demandMax)
          : accent;
      case "quality":
        return ranges
          ? qualityColor(value, ranges.qualityMin ?? 0, ranges.qualityMax ?? 1)
          : accent;
    }
  }

  // Primary value — whichever variable is active, or pressure as default.
  let primaryLabel = "Pressure";
  let primaryValue = "—";
  let primaryColor = accent;
  if (node.pressure != null) {
    primaryLabel = "Pressure";
    primaryValue = formatQty(
      node.pressure,
      "pressure",
      sys,
      sys === "si" ? 2 : undefined,
    );
    primaryColor = valueColor("pressure", node.pressure);
  }
  if (nodeVar === "head" && node.head != null) {
    primaryLabel = "Head";
    primaryValue = formatQty(
      node.head,
      "head",
      sys,
      sys === "si" ? 2 : undefined,
    );
    primaryColor = valueColor("head", node.head);
  }
  if (nodeVar === "demand" && node.demand != null) {
    primaryLabel = "Demand";
    primaryValue = formatQty(
      node.demand,
      "demand",
      sys,
      sys === "si" ? 4 : undefined,
    );
    primaryColor = valueColor("demand", node.demand);
  }
  if (nodeVar === "quality" && node.quality != null) {
    primaryLabel = "Quality";
    primaryValue = node.quality.toFixed(4);
    primaryColor = valueColor("quality", node.quality);
  }

  const secondaries: Array<{ label: string; value: string; color?: string }> =
    [];
  if (nodeVar !== "pressure" && node.pressure != null)
    secondaries.push({
      label: "Pressure",
      value: formatQty(
        node.pressure,
        "pressure",
        sys,
        sys === "si" ? 2 : undefined,
      ),
      color: valueColor("pressure", node.pressure),
    });
  if (nodeVar !== "head" && node.head != null)
    secondaries.push({
      label: "Head",
      value: formatQty(node.head, "head", sys, sys === "si" ? 2 : undefined),
      color: valueColor("head", node.head),
    });
  if (nodeVar !== "demand" && node.demand != null)
    secondaries.push({
      label: "Demand",
      value: formatQty(
        node.demand,
        "demand",
        sys,
        sys === "si" ? 4 : undefined,
      ),
      color: valueColor("demand", node.demand),
    });
  if (nodeVar !== "quality" && node.quality != null)
    secondaries.push({
      label: "Quality",
      value: node.quality.toFixed(4),
      color: valueColor("quality", node.quality),
    });

  return (
    <div
      style={{
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius: 8,
        padding: "14px 12px 12px",
        marginBottom: 14,
        display: "flex",
        flexDirection: "column",
        gap: 12,
      }}
    >
      <BigValue
        label={primaryLabel}
        value={primaryValue}
        color={primaryColor}
      />
      {secondaries.length > 0 && (
        <div
          style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 6 }}
        >
          {secondaries.map((s) => (
            <SecondaryCell
              key={s.label}
              label={s.label}
              value={s.value}
              color={s.color}
            />
          ))}
        </div>
      )}
    </div>
  );
}

export function LinkResultsCard({
  link,
  accent,
  linkVar,
  ranges,
  hasSimulation,
}: {
  link: Link;
  accent: string;
  linkVar?: LinkVariable;
  ranges?: ResultRanges;
  hasSimulation?: boolean;
}) {
  const sys = useUnitSystem();
  const hasSim =
    link.flow != null || link.status != null || link.quality != null;
  if (!hasSim && !hasSimulation) return <EmptyStateCard />;

  function valueColor(variable: LinkVariable, value: number): string {
    if (!linkVar || linkVar !== variable) return accent;
    switch (variable) {
      case "flow":
        return flowColor(value, ranges?.flowMax ?? 0);
      case "velocity":
        return velocityColor(value);
      case "status":
        return statusColor(value);
      case "headloss":
        return accent;
      case "quality":
        return ranges
          ? qualityColor(value, ranges.qualityMin ?? 0, ranges.qualityMax ?? 1)
          : accent;
    }
  }

  let primaryLabel = "Flow";
  let primaryValue = "—";
  let primaryColor = accent;
  if (link.flow != null) {
    primaryLabel = "Flow";
    primaryValue = formatQty(
      link.flow,
      "flow",
      sys,
      sys === "si" ? 2 : undefined,
    );
    primaryColor = valueColor("flow", link.flow);
  }
  if (linkVar === "velocity" && link.velocity != null) {
    primaryLabel = "Velocity";
    primaryValue = formatQty(
      link.velocity,
      "velocity",
      sys,
      sys === "si" ? 3 : undefined,
    );
    primaryColor = valueColor("velocity", link.velocity);
  }
  if (linkVar === "status" && link.status != null) {
    primaryLabel = "Status";
    primaryValue = statusLabel(link.status);
    primaryColor = valueColor("status", link.status);
  }

  const secondaries: Array<{ label: string; value: string; color?: string }> =
    [];
  if (linkVar !== "flow" && link.flow != null)
    secondaries.push({
      label: "Flow",
      value: formatQty(link.flow, "flow", sys, sys === "si" ? 2 : undefined),
      color: valueColor("flow", link.flow),
    });
  if (linkVar !== "velocity" && link.velocity != null)
    secondaries.push({
      label: "Velocity",
      value: formatQty(
        link.velocity,
        "velocity",
        sys,
        sys === "si" ? 3 : undefined,
      ),
      color: valueColor("velocity", link.velocity),
    });
  if (linkVar !== "status")
    secondaries.push({
      label: "Status",
      value: statusLabel(link.status),
      color: link.status != null ? statusColor(link.status) : undefined,
    });
  secondaries.push({
    label: "Quality",
    value: link.quality != null ? link.quality.toFixed(4) : "—",
  });

  return (
    <div
      style={{
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius: 8,
        padding: "14px 12px 12px",
        marginBottom: 14,
        display: "flex",
        flexDirection: "column",
        gap: 12,
      }}
    >
      <BigValue
        label={primaryLabel}
        value={primaryValue}
        color={primaryColor}
      />
      {secondaries.length > 0 && (
        <div
          style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 6 }}
        >
          {secondaries.map((s) => (
            <SecondaryCell
              key={s.label}
              label={s.label}
              value={s.value}
              color={s.color}
            />
          ))}
        </div>
      )}
    </div>
  );
}
