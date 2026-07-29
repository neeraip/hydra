// ── Hover value chip ──────────────────────────────────────────────────────────
// A cursor-following chip showing the hovered element's id and its value for the
// active node/link variable (in display units). Renders nothing until results
// are loaded and something is hovered.

import type { PeriodResults } from "../hooks";
import { elementTypeBadge } from "../types/elementTypes";
import { toDisplay, unitLabel, type useUnitSystem } from "../units";
import { statusLabel } from "./MapCanvas/colorUtils";
import type { LinkVariable, NodeVariable } from "./types";

/** What the hover chip is pointing at. `si` indexes the period-result arrays. */
export interface HoverTip {
  x: number;
  y: number;
  kind: "node" | "link";
  /** Specific element type ("junction", "pump", …) — drives the letter badge.
   * `kind` alone cannot: it only distinguishes nodes from links. */
  type: string;
  si: number;
  id: string;
}

function hoverTipValue(
  tip: HoverTip,
  pr: PeriodResults,
  nodeVar: NodeVariable,
  linkVar: LinkVariable,
  sys: ReturnType<typeof useUnitSystem>,
): string | null {
  const at = (arr: ArrayLike<number> | null | undefined, i: number) =>
    arr && i < arr.length ? arr[i] : null;
  const q = (
    v: number | null,
    kind: Parameters<typeof toDisplay>[1],
    dp: number,
  ) =>
    v == null || !Number.isFinite(v)
      ? null
      : `${toDisplay(v, kind, sys).toFixed(dp)} ${unitLabel(kind, sys)}`;
  const i = tip.si;
  if (tip.kind === "node") {
    switch (nodeVar) {
      case "pressure":
        return q(at(pr.nodePressure, i), "pressure", 1);
      case "head":
        return q(at(pr.nodeHead, i), "head", 1);
      case "demand":
        return q(at(pr.nodeDemand, i), "demand", 2);
      case "quality": {
        const v = at(pr.nodeQuality, i);
        return v == null || !Number.isFinite(v) ? null : v.toFixed(2);
      }
    }
  } else {
    switch (linkVar) {
      case "flow":
        return q(at(pr.linkFlow, i), "flow", 2);
      case "velocity":
        return q(at(pr.linkVelocity, i), "velocity", 2);
      case "headloss":
        return q(at(pr.linkHeadloss, i), "headloss", 2);
      case "status": {
        const s = at(pr.linkStatus, i);
        // These are OUT-file status codes, not a 0/1 open/closed flag: 3 is
        // Open and 2 is Closed, so a home-grown mapping here labelled every
        // ordinary open link "cv" — a value that is not even a simulated
        // status, only a model-side check-valve flag.
        return s == null ? null : statusLabel(s);
      }
      case "quality": {
        const v = at(pr.linkQuality, i);
        return v == null || !Number.isFinite(v) ? null : v.toFixed(2);
      }
    }
  }
  return null;
}

export function HoverChip({
  tip,
  periodResult,
  nodeVar,
  linkVar,
  sys,
}: {
  tip: HoverTip | null;
  periodResult: PeriodResults | null;
  nodeVar: NodeVariable;
  linkVar: LinkVariable;
  sys: ReturnType<typeof useUnitSystem>;
}) {
  if (!tip) return null;
  const value = periodResult
    ? hoverTipValue(tip, periodResult, nodeVar, linkVar, sys)
    : null;
  const badge = elementTypeBadge(tip.type);
  return (
    <div
      style={{
        position: "absolute",
        left: tip.x + 14,
        top: tip.y + 14,
        pointerEvents: "none",
        zIndex: 5,
        background: "var(--bg-panel)",
        border: "1px solid var(--border-hover)",
        borderRadius: 5,
        boxShadow: "var(--shadow-2)",
        padding: "3px 7px",
        fontSize: 11,
        fontFamily: "var(--font-ui)",
        color: "var(--text-primary)",
        whiteSpace: "nowrap",
        maxWidth: 260,
        overflow: "hidden",
        textOverflow: "ellipsis",
      }}
    >
      <span
        style={{
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
          minWidth: 14,
          height: 13,
          padding: "0 2px",
          marginRight: 5,
          borderRadius: 3,
          fontSize: 8.5,
          fontWeight: 700,
          verticalAlign: "text-bottom",
          color: badge.color,
          background: `${badge.color}1f`,
          border: `1px solid ${badge.color}55`,
          boxSizing: "border-box",
        }}
      >
        {badge.label}
      </span>
      <span style={{ fontWeight: 600 }}>{tip.id}</span>
      {value != null && (
        <span style={{ color: "var(--text-secondary)" }}> · {value}</span>
      )}
    </div>
  );
}
