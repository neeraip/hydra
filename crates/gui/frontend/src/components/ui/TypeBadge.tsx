import { elementTypeBadge } from "../../types/elementTypes";

/** Compact letter badge per element type. Letters and colours come from the
 * shared table so the panels, the network list and the canvas hover chip can
 * never disagree; tooltips carry the full name. */
export function TypeBadge({ type }: { type: string }) {
  const meta = elementTypeBadge(type);
  return (
    <span
      data-tooltip={type.charAt(0).toUpperCase() + type.slice(1)}
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        minWidth: 18,
        height: 16,
        padding: "0 3px",
        borderRadius: 4,
        fontSize: "var(--text-2xs)",
        fontWeight: 700,
        letterSpacing: "0.02em",
        fontFamily: "var(--font-ui)",
        color: meta.color,
        background: `${meta.color}1f`,
        border: `1px solid ${meta.color}55`,
        boxSizing: "border-box",
      }}
    >
      {meta.label}
    </span>
  );
}
