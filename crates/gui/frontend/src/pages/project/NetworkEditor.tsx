import { useEffect, useMemo, useState } from "react";
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
import {
  type EditorSection,
  EditorShell,
  EditorStatusBar,
} from "./EditorShell";
import type { Section } from "./NetworkEditor/ElementsEditor";
import { ElementsEditor } from "./NetworkEditor/ElementsEditor";
import {
  COLLECTIONS,
  type CollectionId,
  type EditorSectionId,
  SECTION_FOR_KIND,
  SECTION_LABEL,
} from "./NetworkEditor/editorRail";
import {
  collectDirtyKinds,
  ELEMENT_KIND_ORDER,
  elementCounts,
} from "./NetworkEditor/elementsEditorDerivations";

/**
 * The rail lists every element kind and every collection as one flat
 * inventory, so `Junctions` and `Patterns` are siblings.
 *
 * They were two levels before — an `Elements` entry containing six kinds,
 * beside three collection entries — which made one rail item mean
 * something different from the other three, and pushed the kinds into a
 * horizontal strip that hid them behind an invisible scroll once there
 * were enough. A vertical list scrolls honestly and shows every kind's
 * count without a click.
 *
 * Which kind each section shows and what it is called lives in
 * `NetworkEditor/editorRail.ts`, beside the test pinning it to the
 * engine's §4.2 catalog — that file also records why this rail is
 * declared by hand rather than derived from the catalog the way the
 * drainage rail is. What stays here is what the manifest cannot know:
 * the counts, which fold in unsaved adds and deletes.
 */
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
  const { showToast, editorFocus } = useAppState();
  const {
    dirtyCount,
    dirtyBySection,
    previewPatches,
    discardAll,
    saveAll,
    isSaving,
    elementsDraft,
    pendingAdds,
    pendingDeletes,
  } = useDraft();
  const counts = useMemo(
    () => elementCounts(allNodes, allLinks, pendingAdds, pendingDeletes),
    [allNodes, allLinks, pendingAdds, pendingDeletes],
  );
  const dirtyKinds = useMemo(
    () =>
      collectDirtyKinds({
        draftEntries: Array.from(elementsDraft.values()),
        pendingAdds,
        pendingDeletes,
      }),
    [elementsDraft, pendingAdds, pendingDeletes],
  );

  const collectionCount: Record<CollectionId, number> = {
    curves: curves.length,
    patterns: patterns.length,
    controls: controls.length + rules.length,
  };

  const sections: EditorSection[] = [
    ...ELEMENT_KIND_ORDER.map((kind) => ({
      id: SECTION_FOR_KIND[kind] as EditorSectionId,
      label: SECTION_LABEL[SECTION_FOR_KIND[kind]],
      count: counts[kind],
      dirtyCount: dirtyKinds.has(kind) ? 1 : 0,
      kindId: kind,
    })),
    ...COLLECTIONS.map((c, i) => ({
      id: c.id as EditorSectionId,
      label: c.label,
      count: collectionCount[c.id],
      dirtyCount: dirtyBySection[c.id],
      kindId: c.kindId,
      // The collections are a different sort of thing from the spatial
      // kinds above; one rule parts them rather than a second nav level.
      startsGroup: i === 0,
    })),
  ];

  const [activeSectionId, setActiveSectionId] =
    useState<EditorSectionId>("junctions");
  const [previewOpen, setPreviewOpen] = useState(false);
  const [confirmDiscardOpen, setConfirmDiscardOpen] = useState(false);
  // General "reveal this element" request forwarded to the ElementsEditor:
  // switches to its kind tab, selects the row, and scrolls it into view.
  // Sources: the Curves tab's "attached to" link, and the canvas
  // inspector's "Open in editor" (via AppContext.editorFocus).
  const [elementFocus, setElementFocus] = useState<{
    kind: string;
    id: string;
    token: number;
  } | null>(null);

  function handleNavigateToPump(pumpId: string) {
    setActiveSectionId("pumps");
    setElementFocus({ kind: "pump", id: pumpId, token: Date.now() });
  }

  // Canvas "Open in editor" → reveal the element. `editorFocus.nonce` bumps on
  // every request so re-opening the same element re-runs the jump.
  useEffect(() => {
    if (!editorFocus) return;
    // Reveal the kind's own rail entry, not a generic "elements" tab —
    // the rail is the navigation now.
    const target = SECTION_FOR_KIND[editorFocus.kind];
    if (target) setActiveSectionId(target);
    setElementFocus({
      kind: editorFocus.kind,
      id: editorFocus.id,
      token: editorFocus.nonce,
    });
  }, [editorFocus]);

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

  const validSection: EditorSectionId = sections.some(
    (s) => s.id === activeSectionId,
  )
    ? activeSectionId
    : "junctions";

  const isElementSection = (id: EditorSectionId): id is Section =>
    id !== "curves" && id !== "patterns" && id !== "controls";
  const elementSection: Section = isElementSection(validSection)
    ? validSection
    : "junctions";

  return (
    <EditorShell
      sections={sections}
      activeSectionId={validSection}
      onSelectSection={(id) => setActiveSectionId(id as EditorSectionId)}
      footer={
        <EditorStatusBar tone={dirtyCount > 0 ? "dirty" : "quiet"}>
          {dirtyCount > 0 ? (
            <>
              <span
                style={{ color: "rgba(220, 160, 40, 0.9)", fontWeight: 500 }}
              >
                {dirtyCount} unsaved change{dirtyCount !== 1 ? "s" : ""}
              </span>
              <div style={{ flex: 1 }} />
              <SecondaryButton onClick={() => setPreviewOpen(true)}>
                Preview changes
              </SecondaryButton>
              <SecondaryButton onClick={handleDiscardClick} disabled={isSaving}>
                Discard
              </SecondaryButton>
              <button
                type="button"
                onClick={() => void saveAll()}
                disabled={isSaving}
                style={{
                  padding: "4px 12px",
                  borderRadius: 5,
                  border: "1px solid rgba(220, 160, 40, 0.4)",
                  background: "rgba(220, 160, 40, 0.12)",
                  color: "rgba(220, 160, 40, 0.95)",
                  fontFamily: "var(--font-ui)",
                  fontSize: "var(--text-md)",
                  fontWeight: 500,
                  cursor: isSaving ? "default" : "pointer",
                  opacity: isSaving ? 0.7 : 1,
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
        </EditorStatusBar>
      }
    >
      {/* Every section stays mounted so neither draft data nor per-section
          UI state (selection, sort, expanded rows) is lost when the rail
          moves; only visibility toggles. */}
      <div
        style={{
          display: isElementSection(validSection) ? "flex" : "none",
          flex: 1,
          minHeight: 0,
        }}
      >
        <ElementsEditor
          section={elementSection}
          onSectionChange={setActiveSectionId}
          focusKind={elementFocus?.kind}
          focusId={elementFocus?.id}
          focusToken={elementFocus?.token}
        />
      </div>
      <div
        style={{
          display: validSection === "curves" ? "flex" : "none",
          flex: 1,
          minHeight: 0,
        }}
      >
        <CurveEditor accent={accent} onNavigateToPump={handleNavigateToPump} />
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
    </EditorShell>
  );
}

/** The status bar's neutral action styling, repeated three times before. */
function SecondaryButton({
  onClick,
  disabled,
  children,
}: {
  onClick: () => void;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled}
      className="legend-btn-secondary"
      style={{
        padding: "4px 12px",
        borderRadius: 5,
        border: "1px solid var(--border)",
        background: "transparent",
        color: "var(--text-secondary)",
        fontFamily: "var(--font-ui)",
        fontSize: "var(--text-md)",
        cursor: disabled ? "default" : "pointer",
      }}
    >
      {children}
    </button>
  );
}
