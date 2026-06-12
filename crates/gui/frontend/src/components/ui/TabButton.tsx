import type React from "react";

interface TabButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  active: boolean;
  variant?: "underline" | "fill";
  dirty?: boolean;
  badge?: number;
  badgeColor?: string;
}

export function TabButton({
  active,
  onClick,
  variant = "fill",
  dirty = false,
  badge,
  badgeColor,
  children,
  style,
  ...rest
}: TabButtonProps) {
  const baseStyle: React.CSSProperties =
    variant === "underline"
      ? {
          padding: "8px 14px",
          border: "none",
          background: "transparent",
          color: active ? "var(--text-primary)" : "var(--text-secondary)",
          cursor: "pointer",
          fontSize: 12,
          fontWeight: active ? 600 : 500,
          fontFamily: "var(--font-ui)",
          borderBottom: active
            ? "2px solid var(--accent)"
            : "2px solid transparent",
          transition: "color var(--t-fast), border-color var(--t-fast)",
          whiteSpace: "nowrap",
          display: "inline-flex",
          alignItems: "center",
          gap: 5,
        }
      : {
          border: "none",
          borderRadius: 0,
          padding: "5px 12px",
          fontSize: 12,
          fontFamily: "var(--font-ui)",
          cursor: "pointer",
          background: active ? "var(--accent-dim)" : "transparent",
          color: active ? "var(--accent)" : "var(--text-secondary)",
          transition: "background var(--t-fast), color var(--t-fast)",
          whiteSpace: "nowrap",
          display: "inline-flex",
          alignItems: "center",
          gap: 5,
        };

  return (
    <button
      type="button"
      onClick={onClick}
      style={{ ...baseStyle, ...style }}
      {...rest}
    >
      {children}
      {dirty && (
        <span
          style={{
            width: 6,
            height: 6,
            borderRadius: "50%",
            background: "rgba(220, 160, 40, 0.9)",
            flexShrink: 0,
            display: "inline-block",
          }}
        />
      )}
      {badge !== undefined && (
        <span
          style={{
            fontSize: 10,
            fontWeight: 700,
            background: badgeColor ?? "var(--accent)",
            color: "#fff",
            borderRadius: 4,
            padding: "1px 5px",
            marginLeft: 2,
          }}
        >
          {badge}
        </span>
      )}
    </button>
  );
}
