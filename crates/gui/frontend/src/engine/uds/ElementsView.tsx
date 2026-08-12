/**
 * The urban-drainage Editor view.
 *
 * Same page as the water-distribution editor — the shared `EditorShell`
 * rail, one entry per element kind with its badge and count, one table at
 * a time — with drainage's own kinds and columns inside it. Nothing here
 * names a kind: the rail is built from the engine's own §4.2 catalog, so
 * this file would render an engine it has never heard of.
 *
 * A per-kind table rather than shared Nodes/Links tables, because a shared
 * table can only show columns its kinds have in common, which for drainage
 * is close to nothing — a junction's invert means nothing to an outfall,
 * whose boundary condition means nothing to a storage unit.
 */

import { PencilSquareIcon } from "@heroicons/react/16/solid";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { useCanvasSelection } from "../../canvas/selection-context";
import { DeleteConfirmModal } from "../../components/modals/DeleteConfirmModal";
import { RenameElementModal } from "../../components/modals/RenameElementModal";
import { KindTable } from "../../components/panels/KindTable";
import {
  deleteElement,
  patchNodePosition,
  useCollectionDetail,
  useElementKinds,
  useKindCounts,
  useKindElements,
} from "../../hooks";
import { useElementAttributeWrite } from "../../hooks/useAttributeWrite";
import { useElementRename } from "../../hooks/useElementRename";
import { deletionSummary } from "../../pages/project/CanvasView/deletionSummary";
import {
  type EditorSection,
  EditorShell,
  EditorStatusBar,
} from "../../pages/project/EditorShell";
import { CollectionDetail } from "./CollectionDetail";
import { railGroupBreak } from "./railGroups";

export function UdsElementsView() {
  const { project } = useActiveProject();
  const { activeScenarioId, editorFocus, showToast, setProjectView } =
    useAppState();
  const {
    selectNode,
    selectLink,
    selectRegion,
    selectedNodeId,
    selectedLinkId,
    selectedRegionId,
    zoomToNode,
    zoomToLink,
  } = useCanvasSelection();
  const renameFlow = useElementRename();

  const kinds = useElementKinds(project?.engine);
  const counts = useKindCounts(project?.id, activeScenarioId);

  // Every kind the engine declares gets a rail entry, including the ones
  // this model has none of.
  //
  // Hiding empty kinds looked tidier and cost more than it saved: a rail
  // showing five entries left no way to tell a sparse model from an
  // incomplete application. "This model has no pollutants" is a fact
  // worth reading, and it was indistinguishable from "pollutants cannot
  // be shown". An empty entry is dimmed rather than absent, so what the
  // model *has* still reads at a glance.
  const present = useMemo(
    () => kinds.map((k) => ({ ...k, count: counts[k.id] ?? 0 })),
    [kinds, counts],
  );

  const [activeKind, setActiveKind] = useState<string | null>(null);
  const kind = present.some((k) => k.id === activeKind)
    ? activeKind
    : (present[0]?.id ?? null);
  const activeClass = present.find((k) => k.id === kind)?.class ?? "point";

  // The canvas inspector's "Open in editor" → show the element's own kind
  // and scroll its row into view. The row is already selected: this view
  // reads the canvas selection, and the request always comes from the
  // element that selection names.
  //
  // The rail is keyed by catalog kind id and so is the request, so nothing
  // here maps between the two vocabularies — a kind this file has never
  // heard of reveals correctly.
  const [revealToken, setRevealToken] = useState(0);
  useEffect(() => {
    if (!editorFocus) return;
    setActiveKind(editorFocus.kind);
    // Follows the request's nonce, so opening the same element twice moves
    // the table both times.
    setRevealToken(editorFocus.nonce);
  }, [editorFocus]);

  const { elements, refetch } = useKindElements(
    project?.id,
    activeScenarioId,
    kind,
  );

  // The table redraws from a refetch rather than from what was typed:
  // the backend is the one that knows what the value became, and a cell
  // showing the entered number while the model holds a converted one
  // lies until the next reload.
  const write = useElementAttributeWrite();
  const onEdit = useCallback(
    (
      id: string,
      key: string,
      value: number | string,
      previous?: number | string,
    ) => write(id, key, value, previous).then(refetch),
    [write, refetch],
  );

  // A move is its own operation, not an attribute write: a drainage
  // element's position is a line in a section the engine preserves
  // verbatim, and it appears in no attribute schema.
  const onMove = useCallback(
    (id: string, x: number, y: number) =>
      patchNodePosition(id, x, y)
        .then(refetch)
        .catch((e) => {
          showToast(String(e), "error");
          throw e;
        }),
    [refetch, showToast],
  );

  // Memoised because the reveal action closes over it: a fresh function
  // each render would make that callback fresh too, which is the same as
  // not memoising it at all.
  const select = useCallback(
    (id: string) => {
      if (activeClass === "point") selectNode(id);
      else if (activeClass === "polyline") selectLink(id);
      else if (activeClass === "region") selectRegion(id);
      else setOpenContainer(id);
    },
    [activeClass, selectNode, selectLink, selectRegion],
  );

  // The row actions: find it, name it, remove it. Each is the same
  // operation the canvas inspector offers, reached from the table
  // because the table is where a reader finds the element in the first
  // place.
  const onReveal = useCallback(
    (id: string) => {
      select(id);
      setProjectView("canvas");
      // Deferred so the canvas has activated and its map exists — the
      // same wait the water-distribution editor takes.
      window.setTimeout(() => {
        if (activeClass === "polyline") zoomToLink(id);
        else zoomToNode(id);
      }, 220);
    },
    [activeClass, select, setProjectView, zoomToNode, zoomToLink],
  );

  const [renaming, setRenaming] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<string | null>(null);

  // A container's row reports only its size, so selecting one opens what
  // is actually inside it. A local selection, not the canvas's: a curve
  // has no geometry to highlight.
  const [openContainer, setOpenContainer] = useState<string | null>(null);

  // One rule parts the kinds that sit on the map from the ones that do
  // not, rather than a second level of navigation — the same break the
  // wds editor draws above its collections.
  const groupBreak = railGroupBreak(present.map((k) => k.class));
  const sections: EditorSection[] = present.map((k, i) => ({
    id: k.id,
    label: k.labelPlural,
    count: k.count,
    kindId: k.id,
    startsGroup: i === groupBreak,
  }));

  // The highlight follows whichever selection the visible class owns: a
  // selected conduit must light up its row in the Conduits table, not
  // leave the table looking as though nothing is selected.
  //
  // A collection has no geometry, so it has no canvas selection to follow
  // and none to set — a curve is not a region, and routing it to one
  // would select an unrelated element on the map.
  const spatial = activeClass !== "collection";
  const containerId = spatial ? null : openContainer;
  const detail = useCollectionDetail(
    project?.id,
    activeScenarioId,
    kind,
    containerId,
  );
  const selectedId =
    activeClass === "collection"
      ? openContainer
      : activeClass === "point"
        ? selectedNodeId
        : activeClass === "polyline"
          ? selectedLinkId
          : activeClass === "region"
            ? selectedRegionId
            : null;

  // `present` is empty only before the engine's catalog arrives, which is
  // not the same as an empty model — that is every count being zero.
  // Conflating them would greet a loading project with "no network yet".
  if (present.length === 0) return null;
  if (present.every((k) => k.count === 0)) {
    return (
      <div
        style={{
          padding: 24,
          color: "var(--text-tertiary)",
          fontSize: "var(--text-lg)",
        }}
      >
        This project has no network yet.
      </div>
    );
  }

  return (
    <EditorShell
      sections={sections}
      activeSectionId={kind ?? ""}
      onSelectSection={setActiveKind}
      footer={
        <EditorStatusBar>
          {/* This bar is where the Editor reports the state of editing —
              water distribution says "3 unsaved changes" here, because it
              stages its edits. Drainage does not stage: a committed cell
              is written and saved before the field gives focus back, so
              there is never a count to report. That difference is the
              whole message, and it belongs here rather than beside every
              table.

              It reads as a state rather than a caption: WDS's version
              earns peripheral attention by warming to amber when work is
              staged, and this one never changes, so it leans on the icon
              and a secondary weight instead of tertiary body text. */}
          <span
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 6,
              color: "var(--text-secondary)",
              fontWeight: 500,
            }}
          >
            <PencilSquareIcon
              style={{ width: 12, height: 12, flexShrink: 0 }}
              aria-hidden="true"
            />
            Saved as you edit
          </span>
          {/* Five versions of this line have described what drainage
              editing could not do yet, and all five went stale the week
              after they were written: "edited outside Hydra", "editing
              isn't built yet", "these tables don't edit yet", "adding
              and removing", "adding". Each was true when written and
              each outlived its truth, because a sentence about what is
              missing has to be revisited every time something lands —
              and it never was, until someone read it and believed it.

              So this one describes where an operation lives instead,
              which does not expire: values are edited in these tables,
              and the set of elements is changed on the map, where an
              element is placed by pointing at somewhere to put it. That
              is a fact about the shape of the interface rather than
              about how far it has got. */}
          <span style={{ color: "var(--text-tertiary)" }}>
            Add and remove elements on the map.
          </span>
        </EditorStatusBar>
      }
    >
      {kind && (
        <div
          style={{
            flex: 1,
            display: "flex",
            flexDirection: "column",
            minHeight: 0,
          }}
        >
          <KindTable
            key={kind}
            elements={elements}
            activeId={selectedId}
            onSelect={select}
            onEdit={onEdit}
            onMove={onMove}
            onReveal={spatial ? onReveal : undefined}
            onRename={setRenaming}
            onDelete={setDeleting}
            revealToken={revealToken || undefined}
          />
          {containerId && (
            <CollectionDetail detail={detail} elementId={containerId} />
          )}
        </div>
      )}
      {renaming && kind && (
        <RenameElementModal
          kind={kind}
          id={renaming}
          onSubmit={async (newId) => {
            const target = renaming;
            setRenaming(null);
            if (await renameFlow(kind, target, newId)) {
              refetch();
              select(newId.trim());
            }
          }}
          onClose={() => setRenaming(null)}
        />
      )}
      <DeleteConfirmModal
        open={!!deleting}
        elementKind={kind ?? ""}
        elementId={deleting ?? ""}
        // A drainage vertex takes its conduits with it; a conduit and a
        // subcatchment take nothing.
        takesLinks={activeClass === "point"}
        onConfirm={async () => {
          const target = deleting;
          setDeleting(null);
          if (!target || !kind) return;
          try {
            const removed = await deleteElement(kind, target);
            const summary = deletionSummary(removed);
            if (summary) showToast(summary, "info");
            refetch();
          } catch (err) {
            showToast(`Could not delete ${target}: ${err}`, "error");
          }
        }}
        onCancel={() => setDeleting(null)}
      />
    </EditorShell>
  );
}
