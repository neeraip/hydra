/**
 * An indeterminate progress ring.
 *
 * Paired with a label by every caller rather than standing alone, because
 * the app's reduced-motion reset stops animations outright — a lone
 * spinner would sit frozen mid-turn, which reads as broken rather than as
 * busy. The words carry the meaning; the ring only makes it feel alive
 * where motion is welcome.
 */
export function Spinner({ size = 16 }: { size?: number }) {
  return (
    <span
      aria-hidden
      style={{
        display: "inline-block",
        width: size,
        height: size,
        borderRadius: "50%",
        border: "2px solid var(--border)",
        borderTopColor: "var(--text-secondary)",
        animation: "spin 700ms linear infinite",
      }}
    />
  );
}
