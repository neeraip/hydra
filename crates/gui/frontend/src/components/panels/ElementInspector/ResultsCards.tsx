import {
  flowColor,
  pressureColor,
  qualityColor,
  sequentialColor,
  statusColor,
  velocityColor,
} from "../../../canvas/colors";
import { statusLabel } from "../../../canvas/MapCanvas/colorUtils";
import type { LinkVariable, NodeVariable } from "../../../canvas/types";
import type { Link, Node, ResultRanges } from "../../../hooks";
import { formatQty, useUnitSystem } from "../../../units";
import {
  LINK_SI_DECIMALS,
  linkCardLabel,
  linkCardQuantity,
  linkCardVariables,
} from "./linkCards";
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
          fontSize: "var(--text-md)",
          color: "var(--text-secondary)",
          fontFamily: "var(--font-ui)",
        }}
      >
        Run a simulation to see results
      </span>
    </div>
  );
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

  // Derived from what the link actually carries — see `linkCards`. The
  // hand-written run of `if`s this replaces had lost `headloss` entirely
  // and showed `status` and `quality` even when the run produced neither.
  const { primary, secondaries: secondaryVars } = linkCardVariables(
    link,
    linkVar,
  );

  /** One card's text and colour, whichever variable it is. */
  function cardFor(variable: LinkVariable) {
    const raw = link[variable];
    const label = linkCardLabel(variable, link.type);
    if (variable === "status") {
      return {
        label,
        value: statusLabel(link.status),
        color:
          link.status != null ? valueColor("status", link.status) : undefined,
      };
    }
    if (raw == null) return { label, value: "—", color: undefined };
    if (variable === "quality") {
      return { label, value: raw.toFixed(4), color: valueColor(variable, raw) };
    }
    const quantity = linkCardQuantity(variable, link.type);
    return {
      label,
      value: quantity
        ? formatQty(
            raw,
            quantity,
            sys,
            sys === "si" ? LINK_SI_DECIMALS[variable] : undefined,
          )
        : String(raw),
      color: valueColor(variable, raw),
    };
  }

  const {
    label: primaryLabel,
    value: primaryValue,
    color: primaryColorRaw,
  } = cardFor(primary);
  const primaryColor = primaryColorRaw ?? accent;
  const secondaries = secondaryVars.map(cardFor);

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
