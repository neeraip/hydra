import { ChevronDownIcon } from "@heroicons/react/24/outline";
import type React from "react";
import { forwardRef } from "react";

// ── HistoryButton ─────────────────────────────────────────────────────────────
// Split button: left area = single undo/redo; right caret = dropdown list.

export const HistoryButton = forwardRef<
  HTMLDivElement,
  {
    title: string;
    shortcut: string;
    icon: React.ReactNode;
    labels: string[];
    open: boolean;
    onToggleDropdown: () => void;
    onMain: () => void;
    onJump: (steps: number) => void;
    actionWord: string;
  }
>(function HistoryButton(
  {
    title,
    shortcut,
    icon,
    labels,
    open,
    onToggleDropdown,
    onMain,
    onJump,
    actionWord,
  },
  ref,
) {
  const disabled = labels.length === 0;
  const btnBase: React.CSSProperties = {
    height: 28,
    background: "transparent",
    border: "none",
    color: disabled ? "var(--text-disabled)" : "var(--text-secondary)",
    cursor: disabled ? "not-allowed" : "pointer",
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    transition: "background var(--t-fast), color var(--t-fast)",
    borderRadius: 0,
  };
  return (
    <div ref={ref} style={{ position: "relative", display: "inline-flex" }}>
      {/* Main action */}
      <button
        type="button"
        data-tooltip={disabled ? undefined : `${title} (${shortcut})`}
        data-tooltip-pos="bottom"
        disabled={disabled}
        onClick={onMain}
        style={{ ...btnBase, width: 28, borderRadius: "5px 0 0 5px" }}
        onMouseEnter={(e) => {
          if (!disabled)
            (e.currentTarget as HTMLButtonElement).style.background =
              "var(--bg-card)";
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLButtonElement).style.background =
            "transparent";
        }}
      >
        {icon}
      </button>
      {/* Caret toggle */}
      <button
        type="button"
        disabled={disabled}
        onClick={onToggleDropdown}
        aria-label={`${title} history`}
        style={{
          ...btnBase,
          width: 14,
          paddingRight: 1,
          borderRadius: "0 5px 5px 0",
          borderLeft: disabled ? "none" : "1px solid var(--border)",
        }}
        onMouseEnter={(e) => {
          if (!disabled)
            (e.currentTarget as HTMLButtonElement).style.background =
              "var(--bg-card)";
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLButtonElement).style.background =
            "transparent";
        }}
      >
        <ChevronDownIcon style={{ width: 9, height: 9 }} />
      </button>
      {/* Dropdown */}
      {open && labels.length > 0 && (
        <div
          style={{
            position: "absolute",
            top: "calc(100% + 4px)",
            right: 0,
            minWidth: 200,
            maxWidth: 320,
            maxHeight: 280,
            overflowY: "auto",
            background: "var(--bg-panel)",
            border: "1px solid var(--border)",
            borderRadius: 6,
            boxShadow: "0 8px 24px rgba(0,0,0,0.35)",
            zIndex: 9999,
            padding: "4px 0",
          }}
        >
          {labels.map((label, i) => (
            <button
              type="button"
              key={label}
              onClick={() => onJump(i + 1)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                width: "100%",
                padding: "5px 12px",
                background: "transparent",
                border: "none",
                color: "var(--text-primary)",
                fontSize: 12,
                fontFamily: "var(--font-ui)",
                cursor: "pointer",
                textAlign: "left",
                whiteSpace: "nowrap",
                overflow: "hidden",
                textOverflow: "ellipsis",
                transition: "background var(--t-fast)",
              }}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "var(--bg-hover)";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "transparent";
              }}
            >
              <span
                style={{
                  color: "var(--text-disabled)",
                  fontSize: 11,
                  flexShrink: 0,
                }}
              >
                {i + 1}×
              </span>
              <span style={{ overflow: "hidden", textOverflow: "ellipsis" }}>
                {actionWord} {label}
              </span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
});
