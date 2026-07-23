// ── Shared display primitives for the element inspector ──────────────────────

export function PropRow({
  label,
  value,
  accent,
}: {
  label: string;
  value: string;
  accent?: string;
}) {
  return (
    <tr>
      <td
        style={{
          fontSize: 12,
          color: "var(--text-tertiary)",
          padding: "4px 0",
          width: "45%",
        }}
      >
        {label}
      </td>
      <td
        style={{
          fontSize: 12,
          padding: "4px 0",
          fontFamily: "var(--font-mono)",
          color: accent ?? "var(--text-primary)",
          fontWeight: accent ? 600 : 400,
        }}
      >
        {value}
      </td>
    </tr>
  );
}

/** Large primary result value + label beneath it. */
export function BigValue({
  label,
  value,
  color,
}: {
  label: string;
  value: string;
  color: string;
}) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 2,
      }}
    >
      <span
        style={{
          fontSize: 22,
          fontWeight: 700,
          fontFamily: "var(--font-mono)",
          color,
          lineHeight: 1,
        }}
      >
        {value}
      </span>
      <span
        style={{
          fontSize: 10,
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: "0.07em",
        }}
      >
        {label}
      </span>
    </div>
  );
}

/** Compact 2-column grid cell for secondary result values. */
export function SecondaryCell({
  label,
  value,
  color,
}: {
  label: string;
  value: string;
  color?: string;
}) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        gap: 2,
        padding: "6px 10px",
        background: "rgba(255,255,255,0.04)",
        borderRadius: 6,
      }}
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
          fontSize: 13,
          fontWeight: 600,
          fontFamily: "var(--font-mono)",
          color: color ?? "var(--text-primary)",
        }}
      >
        {value}
      </span>
    </div>
  );
}
