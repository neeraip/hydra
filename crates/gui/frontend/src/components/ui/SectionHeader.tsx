import type React from "react";

interface SectionHeaderProps {
  children: React.ReactNode;
  /**
   * inline=true removes the horizontal padding and bottom padding — for use
   * inside flex rows alongside other elements (e.g. header + action button).
   */
  inline?: boolean;
}

export function SectionHeader({
  children,
  inline = false,
}: SectionHeaderProps) {
  return (
    <div
      style={{
        padding: inline ? "0" : "0 16px 4px",
        fontSize: 11,
        fontWeight: 600,
        letterSpacing: "0.08em",
        textTransform: "uppercase" as const,
        color: "var(--text-tertiary)",
      }}
    >
      {children}
    </div>
  );
}
