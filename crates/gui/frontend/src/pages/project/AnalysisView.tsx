import { useState } from "react";
import { AnalysisPanel } from "./AnalysisPanel";

// ── Tab definitions ──────────────────────────────────────────────────────────

const TABS = [{ id: "analysis", label: "Analysis" }] as const;

type ResultTab = (typeof TABS)[number]["id"];

// ── AnalysisView ─────────────────────────────────────────────────────────────

export function AnalysisView() {
  const [activeTab, setActiveTab] = useState<ResultTab>("analysis");

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        minHeight: 0,
      }}
    >
      {/* Tab bar */}
      <div
        style={{
          flexShrink: 0,
          display: "flex",
          alignItems: "flex-end",
          gap: 2,
          padding: "0 24px",
          borderBottom: "1px solid var(--border)",
          background: "var(--bg-panel)",
        }}
      >
        {TABS.map((tab) => {
          const isActive = tab.id === activeTab;
          return (
            <button
              type="button"
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              style={{
                padding: "10px 16px 9px",
                fontSize: 12,
                fontWeight: isActive ? 600 : 500,
                fontFamily: "var(--font-ui)",
                color: isActive
                  ? "var(--text-primary)"
                  : "var(--text-tertiary)",
                background: "transparent",
                border: "none",
                borderBottom: isActive
                  ? "2px solid var(--accent)"
                  : "2px solid transparent",
                cursor: "pointer",
                transition: "color var(--t-fast), border-color var(--t-fast)",
                whiteSpace: "nowrap",
                marginBottom: -1,
              }}
              onMouseEnter={(e) => {
                if (!isActive)
                  (e.currentTarget as HTMLButtonElement).style.color =
                    "var(--text-secondary)";
              }}
              onMouseLeave={(e) => {
                if (!isActive)
                  (e.currentTarget as HTMLButtonElement).style.color =
                    "var(--text-tertiary)";
              }}
            >
              {tab.label}
            </button>
          );
        })}
      </div>

      {/* Tab content */}
      <div style={{ flex: 1, overflow: "auto", minHeight: 0 }}>
        {activeTab === "analysis" && <AnalysisPanel />}
      </div>
    </div>
  );
}
