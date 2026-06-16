import { XMarkIcon } from "@heroicons/react/16/solid";
import type React from "react";
import { useEffect, useMemo } from "react";
import type { PatchItem } from "../../hooks";

function formatKind(kind: string): string {
  return kind.charAt(0).toUpperCase() + kind.slice(1);
}

function formatField(field: string): string {
  const s = field.replace(/_/g, " ");
  return s.charAt(0).toUpperCase() + s.slice(1);
}

function formatValue(value: number | string): string {
  if (typeof value === "number") {
    const s = value.toPrecision(8).replace(/\.?0+$/, "");
    return s.includes("e") ? String(value) : s;
  }
  return String(value);
}

// ── Component ─────────────────────────────────────────────────────────────────

interface InpDiffModalProps {
  patches: PatchItem[];
  onClose: () => void;
}

export function InpDiffModal({ patches, onClose }: InpDiffModalProps) {
  // Group patches by element kind (sorted for stable display order).
  const grouped = useMemo(() => {
    const map = new Map<string, PatchItem[]>();
    for (const p of patches) {
      const list = map.get(p.kind) ?? [];
      list.push(p);
      map.set(p.kind, list);
    }
    return [...map.entries()].sort(([a], [b]) => a.localeCompare(b));
  }, [patches]);

  // Esc to close.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  const COL_ID: React.CSSProperties = {
    width: "30%",
    padding: "6px 12px",
    fontFamily: "var(--font-mono)",
    fontSize: 12,
    color: "var(--text-primary)",
    borderBottom: "1px solid var(--border)",
  };
  const COL_FIELD: React.CSSProperties = {
    width: "35%",
    padding: "6px 12px",
    fontFamily: "var(--font-ui)",
    fontSize: 12,
    color: "var(--text-secondary)",
    borderBottom: "1px solid var(--border)",
  };
  const COL_VALUE: React.CSSProperties = {
    width: "35%",
    padding: "6px 12px",
    fontFamily: "var(--font-mono)",
    fontSize: 12,
    color: "var(--text-primary)",
    borderBottom: "1px solid var(--border)",
  };
  const TH: React.CSSProperties = {
    padding: "5px 12px",
    fontFamily: "var(--font-ui)",
    fontSize: 11,
    fontWeight: 600,
    textAlign: "left",
    color: "var(--text-tertiary)",
    background: "var(--bg-input)",
    borderBottom: "1px solid var(--border)",
    userSelect: "none",
  };

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: backdrop closes the modal on pointer interaction.
    // biome-ignore lint/a11y/useKeyWithClickEvents: backdrop closes the modal on pointer interaction.
    <div
      onClick={onClose}
      style={{
        position: "fixed",
        inset: 0,
        background: "var(--bg-overlay)",
        zIndex: 300,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        animation: "fadeIn 120ms ease-out",
      }}
    >
      {/* biome-ignore lint/a11y/noStaticElementInteractions: panel only stops backdrop clicks. */}
      <div
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => e.stopPropagation()}
        onClick={(e) => e.stopPropagation()}
        style={{
          width: "min(700px, 92vw)",
          maxHeight: "80vh",
          background: "var(--bg-panel)",
          border: "1px solid var(--border-hover)",
          borderRadius: 12,
          boxShadow: "var(--shadow-3)",
          display: "flex",
          flexDirection: "column",
          animation: "scaleIn 160ms ease-out",
        }}
      >
        {/* Header */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            padding: "12px 16px",
            borderBottom: "1px solid var(--border)",
            flexShrink: 0,
          }}
        >
          <span
            style={{
              fontSize: 14,
              fontWeight: 600,
              color: "var(--text-primary)",
              flex: 1,
            }}
          >
            Preview changes
          </span>
          <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
            {patches.length} staged change{patches.length !== 1 ? "s" : ""}
          </span>
          <button
            type="button"
            onClick={onClose}
            onMouseEnter={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "var(--nav-hover)";
            }}
            onMouseLeave={(e) => {
              (e.currentTarget as HTMLButtonElement).style.background =
                "transparent";
            }}
            style={{
              width: 28,
              height: 28,
              borderRadius: 5,
              background: "transparent",
              border: "none",
              color: "var(--text-secondary)",
              cursor: "pointer",
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <XMarkIcon style={{ width: 14, height: 14 }} />
          </button>
        </div>

        {/* Body */}
        <div
          style={{ flex: 1, overflow: "auto", minHeight: 0, padding: "16px 0" }}
        >
          {patches.length === 0 && (
            <div
              style={{
                padding: "24px 32px",
                color: "var(--text-tertiary)",
                fontSize: 13,
                textAlign: "center",
              }}
            >
              No staged changes.
            </div>
          )}
          {grouped.map(([kind, items], gi) => (
            <div
              key={kind}
              style={{ marginBottom: gi < grouped.length - 1 ? 20 : 0 }}
            >
              {/* Section heading */}
              <div
                style={{
                  padding: "0 16px 6px",
                  fontSize: 11,
                  fontWeight: 600,
                  letterSpacing: "0.06em",
                  textTransform: "uppercase",
                  color: "var(--text-tertiary)",
                }}
              >
                {formatKind(kind)}
              </div>
              <table style={{ width: "100%", borderCollapse: "collapse" }}>
                <thead>
                  <tr>
                    <th style={TH}>ID</th>
                    <th style={TH}>Field</th>
                    <th style={TH}>New value</th>
                  </tr>
                </thead>
                <tbody>
                  {items.map((item) => (
                    <tr
                      key={`${item.id}-${item.field}-${String(item.value)}`}
                      onMouseEnter={(e) => {
                        (
                          e.currentTarget as HTMLTableRowElement
                        ).style.background = "var(--nav-hover)";
                      }}
                      onMouseLeave={(e) => {
                        (
                          e.currentTarget as HTMLTableRowElement
                        ).style.background = "transparent";
                      }}
                    >
                      <td style={COL_ID}>{item.id}</td>
                      <td style={COL_FIELD}>{formatField(item.field)}</td>
                      <td style={COL_VALUE}>{formatValue(item.value)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
