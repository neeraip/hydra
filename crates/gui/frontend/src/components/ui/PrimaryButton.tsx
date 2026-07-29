import type React from "react";

const SIZE_STYLES: Record<"sm" | "md", React.CSSProperties> = {
  sm: { height: 28, padding: "0 12px", fontSize: "var(--text-md)" },
  md: { height: 36, padding: "0 20px", fontSize: "var(--text-xl)" },
};

interface PrimaryButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  /** @default "md" */
  size?: "sm" | "md";
}

export function PrimaryButton({
  size = "md",
  style,
  className,
  children,
  title,
  ...props
}: PrimaryButtonProps) {
  return (
    <button
      type="button"
      className={`btn-run${className ? ` ${className}` : ""}`}
      style={{ ...SIZE_STYLES[size], ...style }}
      data-tooltip={title}
      {...props}
    >
      {children}
    </button>
  );
}
