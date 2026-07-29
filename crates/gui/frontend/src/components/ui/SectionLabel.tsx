import type React from "react";

export function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        fontSize: "var(--text-sm)",
        fontWeight: 600,
        color: "var(--text-tertiary)",
        textTransform: "uppercase",
        letterSpacing: "0.08em",
        marginBottom: 8,
        marginTop: 4,
      }}
    >
      {children}
    </div>
  );
}
