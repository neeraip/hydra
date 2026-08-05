import { elementTypeBadge } from "../../types/elementTypes";

/** Box metrics per size. The **width is fixed**, not minimum: engineering
 * tags mix one and two characters (J vs TK, C vs OF), and a chip that
 * sizes to its text leaves a ragged edge down a table column where the
 * badges are meant to read as one. Both sizes share a width so the
 * two-character tags always fit; only the height differs. */
const SIZES = {
  md: { width: 22, height: 16 },
  sm: { width: 22, height: 13 },
} as const;

/**
 * Compact letter badge per element type — the single renderer for element
 * badges, so the panels, the network list, the editors and the canvas
 * hover chip can never disagree about a kind's letters, colour or shape.
 * Tooltips carry the full name.
 */
export function TypeBadge({
  type,
  size = "md",
}: {
  type: string;
  /** `sm` for inline use beside running text (the hover chip). */
  size?: keyof typeof SIZES;
}) {
  const meta = elementTypeBadge(type);
  const box = SIZES[size];
  return (
    <span
      data-tooltip={type.charAt(0).toUpperCase() + type.slice(1)}
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        width: box.width,
        height: box.height,
        borderRadius: 4,
        fontSize: "var(--text-2xs)",
        fontWeight: 700,
        letterSpacing: "0.02em",
        fontFamily: "var(--font-ui)",
        color: meta.color,
        background: `${meta.color}1f`,
        border: `1px solid ${meta.color}55`,
        boxSizing: "border-box",
        flexShrink: 0,
      }}
    >
      {meta.label}
    </span>
  );
}
