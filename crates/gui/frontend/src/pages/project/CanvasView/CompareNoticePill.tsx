import { XMarkIcon } from "@heroicons/react/16/solid";

/**
 * Floating pill explaining why the scenario comparison can't render
 * (baseline missing results / topology drift). The caller owns the
 * visibility condition and the dismissed flag; this is presentation only.
 */
export function CompareNoticePill({
  notice,
  onDismiss,
}: {
  notice: string;
  onDismiss: () => void;
}) {
  return (
    <div
      style={{
        position: "absolute",
        top: 60,
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 25,
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "6px 10px",
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius: 8,
        boxShadow: "var(--shadow-2)",
      }}
    >
      <span
        style={{
          fontSize: 12,
          color: "var(--text-secondary)",
          fontFamily: "var(--font-ui)",
        }}
      >
        {notice}
      </span>
      <button
        type="button"
        onClick={onDismiss}
        aria-label="Dismiss comparison notice"
        style={{
          border: "none",
          background: "transparent",
          cursor: "pointer",
          display: "inline-flex",
          padding: 2,
          color: "var(--text-tertiary)",
        }}
      >
        <XMarkIcon style={{ width: 12, height: 12 }} />
      </button>
    </div>
  );
}
