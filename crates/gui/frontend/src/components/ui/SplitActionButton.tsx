import { ChevronDownIcon } from "@heroicons/react/16/solid";
import { useRef, useState } from "react";

export function SplitActionButton({
  label,
  onClick,
  menuItems,
  size = "md",
}: {
  label: string;
  onClick: () => void;
  menuItems: { label: string; onClick: () => void }[];
  size?: "sm" | "md";
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  const handleBlur = (e: React.FocusEvent<HTMLDivElement>) => {
    if (!ref.current?.contains(e.relatedTarget as Node)) setOpen(false);
  };

  const sm = size === "sm";
  const radius = sm ? 5 : 7;

  const btnBase: React.CSSProperties = {
    border: "none",
    background: "var(--accent)",
    color: "#fff",
    cursor: "pointer",
    fontSize: sm ? 12 : 13,
    fontWeight: sm ? 600 : 500,
    fontFamily: "var(--font-ui)",
    transition: "opacity var(--t-fast)",
  };

  return (
    <div
      ref={ref}
      onBlur={handleBlur}
      style={{
        position: "relative",
        display: "inline-flex",
        borderRadius: radius,
        overflow: "visible",
      }}
    >
      <button
        onClick={onClick}
        style={{
          ...btnBase,
          padding: sm ? "0 10px" : "9px 16px",
          height: sm ? 28 : undefined,
          borderRadius: `${radius}px 0 0 ${radius}px`,
        }}
        onMouseEnter={(e) => {
          (e.currentTarget as HTMLButtonElement).style.opacity = "0.88";
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLButtonElement).style.opacity = "1";
        }}
      >
        {label}
      </button>

      <div
        style={{
          width: 1,
          background: "rgba(255,255,255,0.25)",
          alignSelf: "stretch",
          flexShrink: 0,
        }}
      />

      <button
        onClick={() => setOpen((v) => !v)}
        aria-label="More actions"
        style={{
          ...btnBase,
          padding: sm ? "0 7px" : "9px 9px",
          height: sm ? 28 : undefined,
          borderRadius: `0 ${radius}px ${radius}px 0`,
          display: "flex",
          alignItems: "center",
        }}
        onMouseEnter={(e) => {
          (e.currentTarget as HTMLButtonElement).style.opacity = "0.88";
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLButtonElement).style.opacity = "1";
        }}
      >
        <ChevronDownIcon style={{ width: 13, height: 13 }} />
      </button>

      {open && (
        <div
          style={{
            position: "absolute",
            top: "calc(100% + 4px)",
            left: 0,
            minWidth: "100%",
            background: "var(--bg-card)",
            border: "1px solid var(--border-hover)",
            borderRadius: 7,
            boxShadow: "0 4px 16px rgba(0,0,0,0.28)",
            zIndex: 200,
            overflow: "hidden",
          }}
        >
          {menuItems.map((item) => (
            <button
              key={item.label}
              onClick={() => {
                item.onClick();
                setOpen(false);
              }}
              style={{
                display: "block",
                width: "100%",
                textAlign: "left",
                padding: "8px 14px",
                border: "none",
                background: "transparent",
                color: "var(--text-primary)",
                fontSize: 13,
                fontFamily: "var(--font-ui)",
                cursor: "pointer",
                whiteSpace: "nowrap",
                transition: "background var(--t-fast)",
              }}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "var(--nav-hover)";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "transparent";
              }}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
