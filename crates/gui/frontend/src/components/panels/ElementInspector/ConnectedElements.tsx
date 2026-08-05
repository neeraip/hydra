import { flowColor, pressureColor } from "../../../canvas/colors";
import { useHoverActions } from "../../../canvas/hover-context";
import type { Link, Node } from "../../../hooks";
import { toDisplay, unitLabel, useUnitSystem } from "../../../units";
import { LINK_TYPE_COLOR } from "./ResultsCards";

// ── Connected-elements section ─────────────────────────────────────────────────

export function ConnectedLink({
  link,
  onLocate,
}: {
  link: Link;
  onLocate: (id: string) => void;
}) {
  const sys = useUnitSystem();
  const { hoverLink, clearHover } = useHoverActions();
  const hasFlow = link.flow != null;
  const flow = link.flow ?? 0;
  return (
    <button
      type="button"
      onClick={() => onLocate(link.id)}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "7px 10px",
        border: "1px solid var(--border)",
        borderRadius: 6,
        background: "var(--bg-card)",
        cursor: "pointer",
        textAlign: "left",
        fontFamily: "var(--font-ui)",
        width: "100%",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.borderColor = "var(--border-hover)";
        hoverLink(link.id);
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.borderColor = "var(--border)";
        clearHover();
      }}
      onFocus={() => hoverLink(link.id)}
      onBlur={() => clearHover()}
    >
      {/* Link type stripe */}
      <span
        style={{
          display: "inline-block",
          width: 14,
          height: 3,
          borderRadius: 2,
          background: LINK_TYPE_COLOR[link.type] ?? "var(--text-secondary)",
          flexShrink: 0,
        }}
      />
      <span
        style={{
          fontSize: "var(--text-sm)",
          fontFamily: "var(--font-mono)",
          color: "var(--text-primary)",
          flex: 1,
          minWidth: 0,
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}
      >
        {link.id}
      </span>
      <span
        style={{
          fontSize: "var(--text-xs)",
          color: "var(--text-tertiary)",
          textTransform: "capitalize",
        }}
      >
        {link.type}
      </span>
      {link.diameter != null && link.diameter > 0 && (
        <span
          style={{
            fontSize: "var(--text-xs)",
            fontFamily: "var(--font-mono)",
            color: "var(--text-secondary)",
          }}
        >
          Ø
          {sys === "si"
            ? `${link.diameter}`
            : toDisplay(link.diameter, "diameter", sys).toFixed(2)}
          {unitLabel("diameter", sys)}
        </span>
      )}
      {hasFlow && (
        <span
          style={{
            fontSize: "var(--text-xs)",
            fontFamily: "var(--font-mono)",
            color: flowColor(flow, 0),
          }}
        >
          {toDisplay(flow, "flow", sys).toFixed(sys === "si" ? 2 : 1)}&thinsp;
          {unitLabel("flow", sys)}
        </span>
      )}
    </button>
  );
}

export function ConnectedNodeChip({
  label,
  nodeId,
  allNodes,
  accent,
  onLocate,
}: {
  /** What this node is to the element being inspected — "From", "To", or
   *  a relationship a link's endpoints do not cover, such as the node a
   *  street inlet captures into. */
  label: string;
  nodeId: string;
  allNodes: Node[];
  accent: string;
  onLocate: (id: string) => void;
}) {
  const { hoverNode, clearHover } = useHoverActions();
  const node = allNodes.find((n) => n.id === nodeId);
  return (
    <button
      type="button"
      onClick={() => onLocate(nodeId)}
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        gap: 3,
        padding: "8px 10px",
        border: "1px solid var(--border)",
        borderRadius: 6,
        background: "var(--bg-card)",
        cursor: "pointer",
        textAlign: "left",
        fontFamily: "var(--font-ui)",
      }}
      onMouseEnter={(e) => {
        e.currentTarget.style.borderColor = accent;
        hoverNode(nodeId);
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.borderColor = "var(--border)";
        clearHover();
      }}
      onFocus={() => hoverNode(nodeId)}
      onBlur={() => clearHover()}
    >
      <span
        style={{
          fontSize: "var(--text-xs)",
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: "0.06em",
        }}
      >
        {label}
      </span>
      <span
        style={{
          fontSize: "var(--text-md)",
          fontFamily: "var(--font-mono)",
          color: "var(--text-primary)",
          fontWeight: 500,
        }}
      >
        {nodeId}
      </span>
      {node?.pressure != null && (
        <span
          style={{
            fontSize: "var(--text-sm)",
            fontFamily: "var(--font-mono)",
            color: pressureColor(node.pressure),
          }}
        >
          {node.pressure.toFixed(1)}&thinsp;m
        </span>
      )}
      {node?.type && (
        <span
          style={{
            fontSize: "var(--text-xs)",
            color: "var(--text-tertiary)",
            textTransform: "capitalize",
          }}
        >
          {node.type}
        </span>
      )}
    </button>
  );
}
