import { XMarkIcon } from "@heroicons/react/16/solid";
import type React from "react";

// ── Inspector header ───────────────────────────────────────────────────────────

export function Header({
  id,
  subtitle,
  badge,
  accentColor,
  onClose,
}: {
  id: string;
  subtitle: string;
  /** Visual icon in the header — a circle dot for nodes, a short line for links. */
  badge: React.ReactNode;
  accentColor: string;
  onClose: () => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "10px 12px",
        borderBottom: "1px solid var(--border)",
        flexShrink: 0,
      }}
    >
      {badge}
      <div style={{ flex: 1, minWidth: 0 }}>
        <div
          style={{
            fontSize: 14,
            fontWeight: 600,
            color: "var(--text-primary)",
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {id}
        </div>
        <div
          style={{
            fontSize: 11,
            color: "var(--text-tertiary)",
            marginTop: 1,
            textTransform: "capitalize",
          }}
        >
          {subtitle}
        </div>
      </div>
      <button
        type="button"
        onClick={onClose}
        data-tooltip="Close inspector"
        style={{
          background: "transparent",
          border: "none",
          color: "var(--text-tertiary)",
          cursor: "pointer",
          padding: 4,
          lineHeight: 1,
          display: "inline-flex",
          alignItems: "center",
          justifyContent: "center",
        }}
      >
        <XMarkIcon style={{ width: 14, height: 14 }} />
      </button>
      {/* Hidden span keeps accentColor in the render tree for future use. */}
      <span style={{ display: "none" }}>{accentColor}</span>
    </div>
  );
}
