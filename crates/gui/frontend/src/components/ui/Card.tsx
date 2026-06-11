import type React from "react";

interface CardProps {
  children: React.ReactNode;
  padding?: number | string;
  borderRadius?: number;
  className?: string;
  style?: React.CSSProperties;
}

export function Card({
  children,
  padding = 16,
  borderRadius = 10,
  className,
  style,
}: CardProps) {
  return (
    <div
      className={className}
      style={{
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius,
        padding,
        ...style,
      }}
    >
      {children}
    </div>
  );
}
