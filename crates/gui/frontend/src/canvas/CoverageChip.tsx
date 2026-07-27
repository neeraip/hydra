// ── Offline-coverage chip ────────────────────────────────────────────────────
// Small non-blocking overlay shown when an offline basemap is selected but the
// tile store has no street-detail coverage for the current viewport. Styled
// after the canvas notice chips (topology-stale notice / HoverChip). The
// visibility decision itself is pure — see `shouldShowCoverageChip` in
// hooks/basemaps.ts — so this component only renders the chip.

export function CoverageChip({ onDownload }: { onDownload: () => void }) {
  return (
    <div
      style={{
        position: "absolute",
        bottom: 16,
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 20,
        display: "flex",
        alignItems: "center",
        gap: 6,
        padding: "6px 10px",
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius: 8,
        boxShadow: "var(--shadow-2)",
        whiteSpace: "nowrap",
      }}
    >
      <span
        style={{
          fontSize: 12,
          color: "var(--text-secondary)",
          fontFamily: "var(--font-ui)",
        }}
      >
        No offline detail here
      </span>
      <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>·</span>
      <button
        type="button"
        onClick={onDownload}
        style={{
          border: "none",
          background: "transparent",
          padding: 0,
          fontSize: 12,
          fontWeight: 500,
          fontFamily: "var(--font-ui)",
          color: "var(--accent)",
          cursor: "pointer",
        }}
      >
        Download this area
      </button>
    </div>
  );
}
