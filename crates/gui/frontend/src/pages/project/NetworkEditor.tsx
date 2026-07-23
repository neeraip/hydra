import { useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { ControlsEditor } from "../../components/editors/ControlsEditor";
import { CurveEditor } from "../../components/editors/CurveEditor";
import { PatternEditor } from "../../components/editors/PatternEditor";
import { DeleteConfirmModal } from "../../components/modals/DeleteConfirmModal";
import { InpDiffModal } from "../../components/modals/InpDiffModal";
import {
  useControls,
  useCurves,
  useLinks,
  useNodes,
  usePatterns,
  useRules,
} from "../../hooks";
import { DraftProvider, useDraft } from "../../hooks/DraftContext";
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
  return (
    <DraftProvider>
      <NetworkEditorInner />
    </DraftProvider>
  );
}

function NetworkEditorInner() {
  const allNodes = useNodes();
  const allLinks = useLinks();
  const curves = useCurves();
  const patterns = usePatterns();
  const controls = useControls();
  const rules = useRules();
  const { accent } = useActiveProject();
  const { showToast } = useAppState();
  const {
    dirtyCount,
    dirtyBySection,
    previewPatches,
    discardAll,
    saveAll,
    isSaving,
  } = useDraft();
  const elementsCount = allNodes.length + allLinks.length;
  const sections = EDITOR_SECTIONS.map((s) => {
    if (s.id === "elements") return { ...s, count: elementsCount };
    if (s.id === "curves") return { ...s, count: curves.length };
    if (s.id === "patterns") return { ...s, count: patterns.length };
    if (s.id === "controls")
      return { ...s, count: controls.length + rules.length };
    return s;
  });

  const [activeSectionId, setActiveSectionId] = useState<EditorSectionId>(
    sections[0].id,
  );
  const [previewOpen, setPreviewOpen] = useState(false);
  const [confirmDiscardOpen, setConfirmDiscardOpen] = useState(false);
  const [pumpFocus, setPumpFocus] = useState<{
    id: string;
    token: number;
  } | null>(null);

  function handleNavigateToPump(pumpId: string) {
    setActiveSectionId("elements");
    setPumpFocus({ id: pumpId, token: Date.now() });
  }

  /** Threshold above which Discard requires an explicit confirmation. */
  const DISCARD_CONFIRM_THRESHOLD = 5;

  function performDiscard() {
    const n = dirtyCount;
    discardAll();
    setConfirmDiscardOpen(false);
    showToast(`${n} change${n === 1 ? "" : "s"} discarded`, "info");
  }

  function handleDiscardClick() {
    if (dirtyCount > DISCARD_CONFIRM_THRESHOLD) setConfirmDiscardOpen(true);
    else performDiscard();
  }

  const validSection = sections.find((s) => s.id === activeSectionId)
    ? activeSectionId
    : sections[0].id;

  const sectionDirty: Record<EditorSectionId, number> = {
    elements: dirtyBySection.elements,
    curves: dirtyBySection.curves,
    patterns: dirtyBySection.patterns,
    controls: dirtyBySection.controls,
  };

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        minHeight: 0,
        animation: "fadeIn 150ms ease-out",
      }}
    >
      <div
        style={{ flex: 1, display: "flex", overflow: "hidden", minHeight: 0 }}
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
                type="button"
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
                  color: active
                    ? "var(--text-primary)"
                    : "var(--text-secondary)",
                  cursor: "pointer",
                  fontSize: 13,
                  fontFamily: "var(--font-ui)",
                  textAlign: "left",
                  transition: "background var(--t-fast)",
                }}
              >
                <span>{s.label}</span>
                <div style={{ display: "flex", alignItems: "center", gap: 5 }}>
                  {sectionDirty[s.id] > 0 && (
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
          {/* All four editors stay mounted so neither draft data nor
              per-tab UI state (selection, expanded rows, etc.) is lost
              when switching tabs — only visibility toggles. */}
          <div
            style={{
              display: validSection === "elements" ? "flex" : "none",
              flex: 1,
              minHeight: 0,
            }}
          >
            <ElementsEditor
              focusPumpId={pumpFocus?.id}
              focusPumpToken={pumpFocus?.token}
            />
          </div>
          <div
            style={{
              display: validSection === "curves" ? "flex" : "none",
              flex: 1,
              minHeight: 0,
            }}
          >
            <CurveEditor
              accent={accent}
              onNavigateToPump={handleNavigateToPump}
            />
          </div>
          <div
            style={{
              display: validSection === "patterns" ? "flex" : "none",
              flex: 1,
              minHeight: 0,
            }}
          >
            <PatternEditor accent={accent} />
          </div>
          <div
            style={{
              display: validSection === "controls" ? "flex" : "none",
              flex: 1,
              minHeight: 0,
            }}
          >
            <ControlsEditor accent={accent} />
          </div>
        </div>
      </div>

      {/* Unified status / save bar — spans all four tabs. */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 8,
          padding: "6px 16px",
          borderTop: `1px solid ${dirtyCount > 0 ? "rgba(220, 160, 40, 0.3)" : "var(--border)"}`,
          flexShrink: 0,
          fontSize: 12,
          background: dirtyCount > 0 ? "rgba(220, 160, 40, 0.07)" : undefined,
          transition: "background 200ms",
        }}
      >
        {dirtyCount > 0 ? (
          <>
            <span style={{ color: "rgba(220, 160, 40, 0.9)", fontWeight: 500 }}>
              {dirtyCount} unsaved change{dirtyCount !== 1 ? "s" : ""}
            </span>
            <div style={{ flex: 1 }} />
            <button
              type="button"
              onClick={() => setPreviewOpen(true)}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "var(--nav-hover)";
                (e.currentTarget as HTMLButtonElement).style.borderColor =
                  "var(--border-hover)";
                (e.currentTarget as HTMLButtonElement).style.color =
                  "var(--text-primary)";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "transparent";
                (e.currentTarget as HTMLButtonElement).style.borderColor =
                  "var(--border)";
                (e.currentTarget as HTMLButtonElement).style.color =
                  "var(--text-secondary)";
              }}
              style={{
                padding: "4px 12px",
                borderRadius: 5,
                border: "1px solid var(--border)",
                background: "transparent",
                color: "var(--text-secondary)",
                fontFamily: "var(--font-ui)",
                fontSize: 12,
                cursor: "pointer",
                transition:
                  "background var(--t-fast), border-color var(--t-fast), color var(--t-fast)",
              }}
            >
              Preview changes
            </button>
            <button
              type="button"
              onClick={handleDiscardClick}
              disabled={isSaving}
              onMouseEnter={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "var(--nav-hover)";
                (e.currentTarget as HTMLButtonElement).style.borderColor =
                  "var(--border-hover)";
                (e.currentTarget as HTMLButtonElement).style.color =
                  "var(--text-primary)";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "transparent";
                (e.currentTarget as HTMLButtonElement).style.borderColor =
                  "var(--border)";
                (e.currentTarget as HTMLButtonElement).style.color =
                  "var(--text-secondary)";
              }}
              style={{
                padding: "4px 12px",
                borderRadius: 5,
                border: "1px solid var(--border)",
                background: "transparent",
                color: "var(--text-secondary)",
                fontFamily: "var(--font-ui)",
                fontSize: 12,
                cursor: "pointer",
                transition:
                  "background var(--t-fast), border-color var(--t-fast), color var(--t-fast)",
              }}
            >
              Discard
            </button>
            <button
              type="button"
              onClick={() => void saveAll()}
              disabled={isSaving}
              onMouseEnter={(e) => {
                if (isSaving) return;
                (e.currentTarget as HTMLButtonElement).style.background =
                  "rgba(220, 160, 40, 0.22)";
                (e.currentTarget as HTMLButtonElement).style.borderColor =
                  "rgba(220, 160, 40, 0.65)";
              }}
              onMouseLeave={(e) => {
                (e.currentTarget as HTMLButtonElement).style.background =
                  "rgba(220, 160, 40, 0.12)";
                (e.currentTarget as HTMLButtonElement).style.borderColor =
                  "rgba(220, 160, 40, 0.4)";
              }}
              style={{
                padding: "4px 12px",
                borderRadius: 5,
                border: "1px solid rgba(220, 160, 40, 0.4)",
                background: "rgba(220, 160, 40, 0.12)",
                color: "rgba(220, 160, 40, 0.95)",
                fontFamily: "var(--font-ui)",
                fontSize: 12,
                fontWeight: 500,
                cursor: isSaving ? "default" : "pointer",
                opacity: isSaving ? 0.7 : 1,
                transition:
                  "background var(--t-fast), border-color var(--t-fast)",
              }}
            >
              {isSaving ? "Saving…" : "Save changes"}
            </button>
          </>
        ) : (
          <span style={{ color: "var(--text-tertiary)" }}>
            No unsaved changes
          </span>
        )}
      </div>

      {previewOpen && (
        <InpDiffModal
          patches={previewPatches}
          onClose={() => setPreviewOpen(false)}
        />
      )}

      {/* Confirm before silently dropping a large batch of staged changes. */}
      <DeleteConfirmModal
        open={confirmDiscardOpen}
        elementKind="changes"
        elementId=""
        title="Discard changes"
        message={
          <>
            Discard{" "}
            <strong style={{ color: "var(--text-primary)" }}>
              {dirtyCount} staged change{dirtyCount === 1 ? "" : "s"}
            </strong>
            ? This cannot be undone.
          </>
        }
        confirmLabel="Discard"
        onCancel={() => setConfirmDiscardOpen(false)}
        onConfirm={performDiscard}
      />
    </div>
  );
}
