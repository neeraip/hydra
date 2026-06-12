import { flowColor, pressureColor } from "../../../canvas/colors";
import type { Link, Node } from "../../../hooks";
import { LINK_TYPE_COLOR } from "./ResultsCards";

// ── Connected-elements section ─────────────────────────────────────────────────

export function ConnectedLink({
  link,
  onLocate,
}: {
  link: Link;
  onLocate: (id: string) => void;
}) {
  const hasFlow = link.flow != null;
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
      onMouseEnter={(e) =>
        (e.currentTarget.style.borderColor = "var(--border-hover)")
      }
      onMouseLeave={(e) =>
        (e.currentTarget.style.borderColor = "var(--border)")
      }
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
          fontSize: 11,
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
          fontSize: 10,
          color: "var(--text-tertiary)",
          textTransform: "capitalize",
        }}
      >
        {link.type}
      </span>
      {link.diameter > 0 && (
        <span
          style={{
            fontSize: 10,
            fontFamily: "var(--font-mono)",
            color: "var(--text-secondary)",
          }}
        >
          Ø{link.diameter}mm
        </span>
      )}
      {hasFlow && (
        <span
          style={{
            fontSize: 10,
            fontFamily: "var(--font-mono)",
            color: flowColor(link.flow!, 0),
          }}
        >
          {link.flow?.toFixed(2)}&thinsp;L/s
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
  label: "From" | "To";
  nodeId: string;
  allNodes: Node[];
  accent: string;
  onLocate: (id: string) => void;
}) {
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
      onMouseEnter={(e) => (e.currentTarget.style.borderColor = accent)}
      onMouseLeave={(e) =>
        (e.currentTarget.style.borderColor = "var(--border)")
      }
    >
      <span
        style={{
          fontSize: 10,
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: "0.06em",
        }}
      >
        {label}
      </span>
      <span
        style={{
          fontSize: 12,
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
            fontSize: 11,
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
            fontSize: 10,
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
