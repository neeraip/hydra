// ── Hover value chip ──────────────────────────────────────────────────────────
// A cursor-following chip showing the hovered element's id and its value for the
// active node/link variable (in display units). Renders nothing until results
// are loaded and something is hovered.

import { TypeBadge } from "../components/ui/TypeBadge";
import { formatGenericValue, type PeriodResults } from "../hooks";
import { toDisplay, unitLabel, type useUnitSystem } from "../units";
import { statusLabel } from "./MapCanvas/colorUtils";
import type { GenericCanvasResults, LinkVariable, NodeVariable } from "./types";

/** What the hover chip is pointing at. `si` indexes the period-result arrays. */
export interface HoverTip {
  x: number;
  y: number;
  /** Which class of element, and therefore which result channel the index
   * below belongs to. A region's index is its position in the region
   * array, which is a different sequence from the node and link ones —
   * reading it against the wrong channel prints a real number for the
   * wrong element, which is worse than printing nothing. */
  kind: "node" | "link" | "region";
  /** Specific element type ("junction", "pump", "subcatchment", …) — drives
   * the letter badge. `kind` alone cannot: it only distinguishes classes. */
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
  // The water-distribution channels have no areal class at all, and the
  // `else` below is the link branch — so without this a subcatchment's
  // index would be read against `linkFlow` and the chip would report
  // another element's flow as if it were the catchment's.
  if (tip.kind === "region") return null;
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

/** Value line for the engine-generic channels: the hovered element's value
 * for its class's selected variable, converted to the active display
 * system with the engine's quantity descriptor. */
function genericTipValue(
  tip: HoverTip,
  generic: GenericCanvasResults,
  sys: ReturnType<typeof useUnitSystem>,
): string | null {
  const channel =
    tip.kind === "node"
      ? generic.node
      : tip.kind === "link"
        ? generic.link
        : generic.region;
  const v = channel?.values?.[tip.si];
  if (channel == null || v == null || !Number.isFinite(v)) return null;
  return formatGenericValue(v, channel.variable.quantity, sys);
}

export function HoverChip({
  tip,
  periodResult,
  generic = null,
  nodeVar,
  linkVar,
  sys,
}: {
  tip: HoverTip | null;
  periodResult: PeriodResults | null;
  generic?: GenericCanvasResults | null;
  nodeVar: NodeVariable;
  linkVar: LinkVariable;
  sys: ReturnType<typeof useUnitSystem>;
}) {
  if (!tip) return null;
  const value = generic
    ? genericTipValue(tip, generic, sys)
    : periodResult
      ? hoverTipValue(tip, periodResult, nodeVar, linkVar, sys)
      : null;
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
        fontSize: "var(--text-sm)",
        fontFamily: "var(--font-ui)",
        color: "var(--text-primary)",
        whiteSpace: "nowrap",
        maxWidth: 260,
        overflow: "hidden",
        textOverflow: "ellipsis",
      }}
    >
      {/* Wrapper carries the spacing and baseline alignment the chip's
          inline layout needs; the badge itself stays metric-identical to
          the one the panels render. */}
      <span style={{ marginRight: 5, verticalAlign: "text-bottom" }}>
        <TypeBadge type={tip.type} size="sm" />
      </span>
      <span style={{ fontWeight: 600 }}>{tip.id}</span>
      {value != null && (
        <span style={{ color: "var(--text-secondary)" }}> · {value}</span>
      )}
    </div>
  );
}
