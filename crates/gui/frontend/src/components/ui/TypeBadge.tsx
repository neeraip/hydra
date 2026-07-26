/** Compact letter badge per element type — colours match the canvas' type
 * colours (reservoir blue, tank green, pump amber; the rest neutral). Pump is
 * "Pu" because pipe already owns "P" and colour alone must not be the only
 * differentiator. Tooltips carry the full name. */
const TYPE_BADGE_META: Record<string, { label: string; color: string }> = {
  junction: { label: "J", color: "#8a93a3" },
  reservoir: { label: "R", color: "#4a90d9" },
  tank: { label: "T", color: "#3daf75" },
  pipe: { label: "P", color: "#8a93a3" },
  pump: { label: "Pu", color: "#d4a017" },
  valve: { label: "V", color: "#8a93a3" },
};

export function TypeBadge({ type }: { type: string }) {
  const meta = TYPE_BADGE_META[type] ?? {
    label: type.charAt(0).toUpperCase(),
    color: "#8a93a3",
  };
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
        fontSize: 9.5,
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
