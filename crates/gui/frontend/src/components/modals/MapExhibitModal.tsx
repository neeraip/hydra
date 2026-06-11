/* Map exhibit generator modal.
   Three columns: theme/style/scope picker (left rail) → live SVG preview
   (center) → details panel (right). Footer inserts the exhibit into the
   active report section. */

import { XMarkIcon } from "@heroicons/react/16/solid";
import { useEffect, useMemo, useState } from "react";
import {
  defaultExhibit,
  type ExhibitScope,
  type ExhibitSpec,
  type ExhibitStyle,
  type ExhibitTheme,
  SCOPE_SPECS,
  STYLE_SPECS,
  THEMES,
} from "../../hooks";
import { MapExhibitPreview } from "./MapExhibitPreview";

interface Props {
  accent: string;
  /** Initial spec to edit, or null to start from a default pressure exhibit. */
  initial?: ExhibitSpec | null;
  /** Section IDs available for placement (id, label). */
  sections: { id: string; label: string }[];
  onClose: () => void;
  onInsert: (spec: ExhibitSpec) => void;
}

export function MapExhibitModal({
  accent,
  initial,
  sections,
  onClose,
  onInsert,
}: Props) {
  const [spec, setSpec] = useState<ExhibitSpec>(
    initial ?? defaultExhibit("pressure"),
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const theme = THEMES[spec.theme];
  const themeIds = useMemo(() => Object.keys(THEMES) as ExhibitTheme[], []);

  function patch(p: Partial<ExhibitSpec>) {
    setSpec((s) => ({ ...s, ...p }));
  }

  return (
    <div
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 1000,
        background: "var(--bg-overlay, rgba(0,0,0,0.55))",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        animation: "fadeIn 120ms ease-out",
      }}
    >
      <div
        style={{
          width: "min(1080px, 96vw)",
          height: "min(720px, 92vh)",
          background: "var(--bg-panel)",
          border: "1px solid var(--border)",
          borderRadius: 8,
          boxShadow: "var(--shadow-2)",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
        }}
      >
        {/* Header */}
        <div
          style={{
            height: 48,
            padding: "0 18px",
            borderBottom: "1px solid var(--border)",
            display: "flex",
            alignItems: "center",
            gap: 12,
            flexShrink: 0,
          }}
        >
          <div
            style={{
              fontSize: 14,
              fontWeight: 600,
              color: "var(--text-primary)",
            }}
          >
            {initial ? "Edit map exhibit" : "Insert map exhibit"}
          </div>
          <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
            · {theme.label} ·{" "}
            {STYLE_SPECS.find((s) => s.id === spec.style)?.label}
          </span>
          <div style={{ flex: 1 }} />
          <button
            onClick={onClose}
            aria-label="Close"
            style={{
              background: "transparent",
              border: "none",
              color: "var(--text-tertiary)",
              cursor: "pointer",
              padding: 4,
              fontSize: 18,
              lineHeight: 1,
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
          style={{ flex: 1, display: "flex", overflow: "hidden", minHeight: 0 }}
        >
          {/* Left rail: theme + style + scope */}
          <div
            style={{
              width: 240,
              flexShrink: 0,
              borderRight: "1px solid var(--border)",
              overflow: "auto",
              padding: "10px 0",
            }}
          >
            <RailHeader>Theme</RailHeader>
            {themeIds.map((id) => {
              const t = THEMES[id];
              const sel = spec.theme === id;
              return (
                <RailItem
                  key={id}
                  active={sel}
                  accent={accent}
                  onClick={() =>
                    patch({
                      theme: id,
                      title: `${t.label} — ${captureScope(spec.scope)}`,
                    })
                  }
                >
                  <span style={{ fontSize: 12, fontWeight: sel ? 600 : 400 }}>
                    {t.label}
                  </span>
                  <span
                    style={{
                      fontSize: 10,
                      color: "var(--text-tertiary)",
                      marginLeft: 8,
                    }}
                  >
                    {t.unit}
                  </span>
                </RailItem>
              );
            })}

            <RailHeader>Style</RailHeader>
            {STYLE_SPECS.map((s) => (
              <RailItem
                key={s.id}
                active={spec.style === s.id}
                accent={accent}
                onClick={() => patch({ style: s.id as ExhibitStyle })}
              >
                <span style={{ fontSize: 12 }}>{s.label}</span>
              </RailItem>
            ))}

            <RailHeader>Scope</RailHeader>
            {SCOPE_SPECS.map((s) => (
              <RailItem
                key={s.id}
                active={spec.scope === s.id}
                accent={accent}
                onClick={() => patch({ scope: s.id as ExhibitScope })}
              >
                <span style={{ fontSize: 12 }}>{s.label}</span>
              </RailItem>
            ))}
          </div>

          {/* Center: preview */}
          <div
            style={{
              flex: 1,
              minWidth: 0,
              overflow: "auto",
              background: "color-mix(in srgb, var(--bg-app) 80%, transparent)",
              padding: 24,
              display: "flex",
              flexDirection: "column",
              gap: 12,
            }}
          >
            <div
              style={{
                fontSize: 12,
                color: "var(--text-tertiary)",
                fontFamily: "var(--font-ui)",
              }}
            >
              Preview
            </div>
            <div style={{ flexShrink: 0 }}>
              <MapExhibitPreview
                spec={spec}
                accent={accent}
                width={680}
                framed
              />
            </div>
            <div
              style={{
                padding: "8px 12px",
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                borderRadius: 5,
                fontSize: 12,
                color: "var(--text-secondary)",
                lineHeight: 1.55,
              }}
            >
              <strong style={{ color: "var(--text-primary)", fontWeight: 600 }}>
                {spec.title}
              </strong>
              <div style={{ marginTop: 4 }}>{spec.caption}</div>
              <div
                style={{
                  marginTop: 6,
                  fontSize: 11,
                  color: "var(--text-tertiary)",
                }}
              >
                {STYLE_SPECS.find((s) => s.id === spec.style)?.desc}
              </div>
            </div>
          </div>

          {/* Right: details */}
          <div
            style={{
              width: 280,
              flexShrink: 0,
              borderLeft: "1px solid var(--border)",
              overflow: "auto",
              padding: 16,
              display: "flex",
              flexDirection: "column",
              gap: 14,
            }}
          >
            <FieldGroup label="Title">
              <input
                value={spec.title}
                onChange={(e) => patch({ title: e.target.value })}
                style={inputStyle}
              />
            </FieldGroup>
            <FieldGroup label="Caption">
              <textarea
                value={spec.caption}
                onChange={(e) => patch({ caption: e.target.value })}
                rows={3}
                style={{
                  ...inputStyle,
                  fontFamily: "var(--font-ui)",
                  resize: "vertical",
                  padding: 6,
                }}
              />
            </FieldGroup>

            <FieldGroup label="Annotations">
              <Toggle
                label="Legend"
                value={spec.showLegend}
                onChange={(v) => patch({ showLegend: v })}
                accent={accent}
              />
              <Toggle
                label="Scale bar"
                value={spec.showScale}
                onChange={(v) => patch({ showScale: v })}
                accent={accent}
              />
              <Toggle
                label="North arrow"
                value={spec.showNorth}
                onChange={(v) => patch({ showNorth: v })}
                accent={accent}
              />
            </FieldGroup>

            <FieldGroup label="Callouts">
              <CalloutsEditor
                callouts={spec.callouts}
                onChange={(c) => patch({ callouts: c })}
                accent={accent}
              />
            </FieldGroup>

            <FieldGroup label="Insert into section">
              <select
                value={spec.sectionId ?? ""}
                onChange={(e) => patch({ sectionId: e.target.value || null })}
                style={inputStyle}
              >
                <option value="">— end of report —</option>
                {sections.map((s) => (
                  <option key={s.id} value={s.id}>
                    {s.label}
                  </option>
                ))}
              </select>
            </FieldGroup>
          </div>
        </div>

        {/* Footer */}
        <div
          style={{
            height: 56,
            padding: "0 18px",
            borderTop: "1px solid var(--border)",
            display: "flex",
            alignItems: "center",
            gap: 10,
            flexShrink: 0,
          }}
        >
          <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
            Theme uses live simulation results. Re-render after re-running.
          </span>
          <div style={{ flex: 1 }} />
          <button
            onClick={onClose}
            style={{
              border: "1px solid var(--border)",
              background: "transparent",
              color: "var(--text-secondary)",
              padding: "6px 14px",
              borderRadius: 5,
              cursor: "pointer",
              fontSize: 13,
              fontFamily: "var(--font-ui)",
            }}
          >
            Cancel
          </button>
          <button
            onClick={() => onInsert(spec)}
            style={{
              background: accent,
              color: "var(--bg-app)",
              border: "none",
              padding: "7px 16px",
              borderRadius: 5,
              cursor: "pointer",
              fontSize: 13,
              fontFamily: "var(--font-ui)",
              fontWeight: 500,
            }}
          >
            {initial ? "Update exhibit" : "Insert exhibit"}
          </button>
        </div>
      </div>
    </div>
  );
}

/* ── helpers ───────────────────────────────────────────────────────────────── */

function captureScope(scope: ExhibitScope): string {
  switch (scope) {
    case "whole":
      return "Whole Network";
    case "selection":
      return "Selection";
    case "south-side":
      return "South Side";
    case "north-feed":
      return "North Feed";
  }
}

function RailHeader({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        padding: "10px 14px 4px",
        fontSize: 10,
        fontWeight: 600,
        color: "var(--text-tertiary)",
        textTransform: "uppercase",
        letterSpacing: 0.5,
      }}
    >
      {children}
    </div>
  );
}
function RailItem({
  active,
  accent,
  onClick,
  children,
}: {
  active: boolean;
  accent: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      style={{
        display: "flex",
        alignItems: "center",
        width: "100%",
        padding: "6px 14px",
        border: "none",
        textAlign: "left",
        background: active ? `${accent}1f` : "transparent",
        borderLeft: active ? `2px solid ${accent}` : "2px solid transparent",
        color: active ? accent : "var(--text-secondary)",
        cursor: "pointer",
        fontFamily: "var(--font-ui)",
      }}
    >
      {children}
    </button>
  );
}
function FieldGroup({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
      <span
        style={{
          fontSize: 10,
          fontWeight: 600,
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: 0.4,
        }}
      >
        {label}
      </span>
      {children}
    </div>
  );
}
function Toggle({
  label,
  value,
  onChange,
  accent,
}: {
  label: string;
  value: boolean;
  onChange: (v: boolean) => void;
  accent: string;
}) {
  return (
    <label
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "5px 8px",
        borderRadius: 4,
        cursor: "pointer",
        fontSize: 12,
        color: "var(--text-secondary)",
        background: value ? `${accent}14` : "transparent",
      }}
    >
      <span>{label}</span>
      <input
        type="checkbox"
        checked={value}
        onChange={(e) => onChange(e.target.checked)}
        style={{ accentColor: accent }}
      />
    </label>
  );
}

function CalloutsEditor({
  callouts,
  onChange,
  accent,
}: {
  callouts: ExhibitSpec["callouts"];
  onChange: (c: ExhibitSpec["callouts"]) => void;
  accent: string;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 5 }}>
      {callouts.length === 0 && (
        <div
          style={{
            fontSize: 11,
            color: "var(--text-tertiary)",
            padding: "4px 0",
          }}
        >
          No callouts. Add one to highlight a node.
        </div>
      )}
      {callouts.map((c, i) => (
        <div key={c.id} style={{ display: "flex", gap: 4 }}>
          <input
            value={c.nodeId}
            placeholder="Node"
            onChange={(e) => {
              const next = [...callouts];
              next[i] = { ...c, nodeId: e.target.value };
              onChange(next);
            }}
            style={{ ...inputStyle, width: 64, fontFamily: "var(--font-mono)" }}
          />
          <input
            value={c.text}
            placeholder="Text"
            onChange={(e) => {
              const next = [...callouts];
              next[i] = { ...c, text: e.target.value };
              onChange(next);
            }}
            style={{ ...inputStyle, flex: 1 }}
          />
          <button
            onClick={() => onChange(callouts.filter((_, j) => j !== i))}
            style={{
              background: "transparent",
              border: "1px solid var(--border)",
              color: "var(--text-tertiary)",
              borderRadius: 4,
              padding: "0 8px",
              fontSize: 11,
              cursor: "pointer",
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <XMarkIcon style={{ width: 14, height: 14 }} />
          </button>
        </div>
      ))}
      <button
        onClick={() =>
          onChange([
            ...callouts,
            {
              id: `c${callouts.length + 1}-${Date.now().toString(36)}`,
              nodeId: "",
              text: "Note",
            },
          ])
        }
        style={{
          alignSelf: "flex-start",
          marginTop: 4,
          background: "transparent",
          border: `1px dashed ${accent}66`,
          color: accent,
          borderRadius: 4,
          padding: "3px 10px",
          fontSize: 11,
          cursor: "pointer",
          fontFamily: "var(--font-ui)",
        }}
      >
        + Add callout
      </button>
    </div>
  );
}

const inputStyle: React.CSSProperties = {
  width: "100%",
  height: 28,
  background: "var(--bg-input, var(--bg-card))",
  border: "1px solid var(--border)",
  color: "var(--text-primary)",
  borderRadius: 4,
  padding: "0 8px",
  fontFamily: "var(--font-ui)",
  fontSize: 12,
  outline: "none",
};
