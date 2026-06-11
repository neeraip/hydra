import { useState } from "react";
import { useActiveProject } from "../../AppContext";
import { ControlsEditor } from "../../components/editors/ControlsEditor";
import { CurveEditor } from "../../components/editors/CurveEditor";
import { PatternEditor } from "../../components/editors/PatternEditor";
import { useLinks, useNodes } from "../../hooks";
import { ElementsEditor } from "./NetworkEditor/ElementsEditor";

type EditorSectionId = "elements" | "curves" | "patterns" | "controls";

interface EditorSectionSpec {
  id: EditorSectionId;
  label: string;
  count: number;
}

const EDITOR_SECTIONS: EditorSectionSpec[] = [
  { id: "elements", label: "Elements", count: 0 },
  { id: "curves", label: "Pump curves", count: 0 },
  { id: "patterns", label: "Patterns", count: 0 },
  { id: "controls", label: "Controls", count: 0 },
];

export function NetworkEditor() {
  const allNodes = useNodes();
  const allLinks = useLinks();
  const { accent } = useActiveProject();
  const elementsCount = allNodes.length + allLinks.length;
  const sections = EDITOR_SECTIONS.map((s) =>
    s.id === "elements" ? { ...s, count: elementsCount } : s,
  );

  const [activeSectionId, setActiveSectionId] = useState<EditorSectionId>(
    sections[0].id,
  );
  const [elementsDraftSize, setElementsDraftSize] = useState(0);

  const validSection = sections.find((s) => s.id === activeSectionId)
    ? activeSectionId
    : sections[0].id;

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        overflow: "hidden",
        minHeight: 0,
        animation: "fadeIn 150ms ease-out",
      }}
    >
      <div
        style={{
          width: 180,
          flexShrink: 0,
          background: "var(--bg-panel)",
          borderRight: "1px solid var(--border)",
          display: "flex",
          flexDirection: "column",
          overflow: "auto",
          paddingTop: 8,
        }}
      >
        {sections.map((s) => {
          const active = s.id === validSection;
          return (
            <button
              key={s.id}
              onClick={() => setActiveSectionId(s.id)}
              onMouseEnter={(e) => {
                if (!active)
                  (e.currentTarget as HTMLButtonElement).style.background =
                    "rgba(255,255,255,0.05)";
              }}
              onMouseLeave={(e) => {
                if (!active)
                  (e.currentTarget as HTMLButtonElement).style.background =
                    "transparent";
              }}
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                width: "100%",
                padding: "8px 14px",
                border: "none",
                background: active ? "var(--accent-dim)" : "transparent",
                borderLeft: active
                  ? "2px solid var(--accent)"
                  : "2px solid transparent",
                color: active ? "var(--text-primary)" : "var(--text-secondary)",
                cursor: "pointer",
                fontSize: 13,
                fontFamily: "var(--font-ui)",
                textAlign: "left",
                transition: "background var(--t-fast)",
              }}
            >
              <span>{s.label}</span>
              <div style={{ display: "flex", alignItems: "center", gap: 5 }}>
                {s.id === "elements" && elementsDraftSize > 0 && (
                  <span
                    style={{
                      width: 6,
                      height: 6,
                      borderRadius: "50%",
                      background: "rgba(220, 160, 40, 0.9)",
                      display: "inline-block",
                      flexShrink: 0,
                    }}
                  />
                )}
                <span
                  style={{
                    fontSize: 11,
                    fontFamily: "var(--font-mono)",
                    color: active ? "var(--accent)" : "var(--text-tertiary)",
                  }}
                >
                  {s.count}
                </span>
              </div>
            </button>
          );
        })}
      </div>

      <div
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          minHeight: 0,
        }}
      >
        {validSection === "elements" && (
          <ElementsEditor onDraftSizeChange={setElementsDraftSize} />
        )}
        {validSection === "curves" && <CurveEditor accent={accent} />}
        {validSection === "patterns" && <PatternEditor accent={accent} />}
        {validSection === "controls" && <ControlsEditor accent={accent} />}
      </div>
    </div>
  );
}
