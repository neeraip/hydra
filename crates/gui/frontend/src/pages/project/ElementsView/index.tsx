/**
 * The Editor view, for whichever engine holds the model.
 *
 * The `EditorShell` rail with one entry per element kind, and one table
 * at a time inside it. Nothing here names a kind, a class or an engine:
 * the rail is built from the engine's §4.2 catalog, the columns from its
 * §4.4 schemas, and every edit goes through the §4.5 operations — so
 * this file renders an engine it has never heard of.
 *
 * It was the drainage editor, and it is here because that turned out to
 * be a fact about which engine's editor was written second rather than
 * about drainage. The water-distribution editor it replaces had six
 * hand-written tables and its own staged-save model; the difference
 * reached the screen, which is what the editing contract exists to
 * prevent.
 *
 * A per-kind table rather than shared Nodes/Links tables, because a
 * shared table can only show columns its kinds have in common, which for
 * drainage is close to nothing — a junction's invert means nothing to an
 * outfall, whose boundary condition means nothing to a storage unit.
 */

import { PencilSquareIcon } from "@heroicons/react/16/solid";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useActiveProject, useAppState } from "../../../AppContext";
import { useCanvasSelection } from "../../../canvas/selection-context";
import { DeleteConfirmModal } from "../../../components/modals/DeleteConfirmModal";
import { RenameElementModal } from "../../../components/modals/RenameElementModal";
import { KindTable } from "../../../components/panels/KindTable";
import { engineComponents } from "../../../engine/registry";
import {
  deleteElement,
  patchNodePosition,
  useCollectionDetail,
  useElementKinds,
  useKindCounts,
  useKindElements,
  useReferenceIds,
} from "../../../hooks";
import {
  useCollectionContentsWrite,
  useElementAttributeWrite,
  useElementEndsWrite,
} from "../../../hooks/useAttributeWrite";
import { useElementRename } from "../../../hooks/useElementRename";
import { deletionSummary } from "../CanvasView/deletionSummary";
import {
  type EditorSection,
  EditorShell,
  EditorStatusBar,
} from "../EditorShell";
import { CollectionDetail } from "./CollectionDetail";
import { railGroupBreak } from "./railGroups";

export function ElementsView() {
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
    clearSelection,
    zoomToNode,
    zoomToLink,
  } = useCanvasSelection();
  const renameFlow = useElementRename();

  const kinds = useElementKinds(project?.engine);
  const { CreateNodeModal: CreateNode } = engineComponents(project?.engine);
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
  // An element to bring into view once the table holds it.
  //
  // Two things ask for this and neither can act immediately: the
  // inspector's "Open in editor" arrives before the kind's rows have
  // been fetched, and a create arrives before the refetch that will
  // contain the new element. So the request is recorded and spent when
  // the row actually exists, rather than fired at a table that does not
  // have it yet and silently doing nothing.
  const [revealId, setRevealId] = useState<string | null>(null);
  const [revealToken, setRevealToken] = useState(0);
  useEffect(() => {
    if (!editorFocus) return;
    setActiveKind(editorFocus.kind);
    setRevealId(editorFocus.id);
  }, [editorFocus]);

  const { elements, refetch } = useKindElements(
    project?.id,
    activeScenarioId,
    kind,
  );

  // The ids a reference column may name. Fetched here rather than in
  // the table, which draws what it is given — and only for the kinds
  // this kind's columns actually reference, which is usually none.
  const referenced = useMemo(
    () => [...new Set(elements.columns.flatMap((c) => c.references ?? []))],
    [elements.columns],
  );
  const referenceIds = useReferenceIds(
    project?.id,
    activeScenarioId,
    referenced,
  );

  useEffect(() => {
    if (revealId == null || !elements.ids.includes(revealId)) return;
    setRevealId(null);
    // A token rather than the id, because the same element can be asked
    // for twice running and the table has to move both times.
    setRevealToken((t) => t + 1);
  }, [revealId, elements.ids]);

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

  // Reconnecting is its own operation too, for the same reason a move
  // is: an end is implied by the polyline class rather than declared by
  // a schema, so it has no key an attribute write could address.
  const writeEnds = useElementEndsWrite();
  const onReconnect = useCallback(
    (id: string, fromId: string, toId: string) => {
      const previous = elements.ends[elements.ids.indexOf(id)];
      return writeEnds(id, fromId, toId, previous).then(refetch);
    },
    [writeEnds, refetch, elements.ends, elements.ids],
  );

  // The ids either end may name: every point in the model, whatever kind
  // of point. Not a §4.5.1.1 reference — an end names no single declared
  // kind — so the list is built from the class catalog rather than from
  // a column's `references`.
  const pointKinds = useMemo(
    () =>
      activeClass === "polyline"
        ? kinds.filter((k) => k.class === "point").map((k) => k.id)
        : [],
    [kinds, activeClass],
  );
  const endsByKind = useReferenceIds(project?.id, activeScenarioId, pointKinds);
  const endIds = useMemo(
    () => Object.values(endsByKind).flat().sort(),
    [endsByKind],
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
  const [adding, setAdding] = useState(false);

  // The catalog's answer, not this file's: a kind that needs a relation
  // curve says so, and the button is simply absent rather than present
  // and refusing.
  const creatableHere = present.find((k) => k.id === kind)?.creatable ?? false;

  /** A free id for a new element of `newKind`, from the ids in view. */
  const suggestId = useCallback(
    (newKind: string) => {
      const prefix = newKind.slice(0, 1).toUpperCase();
      const taken = new Set(elements.ids);
      for (let i = 1; i <= 9999; i += 1) {
        const candidate = `${prefix}${i}`;
        if (!taken.has(candidate)) return candidate;
      }
      return `${prefix}${elements.ids.length + 1}`;
    },
    [elements.ids],
  );

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
  const { detail, refetch: refetchDetail } = useCollectionDetail(
    project?.id,
    activeScenarioId,
    kind,
    containerId,
  );

  // A container's contents are their own operation too (§4.5.2.2): a
  // curve's points are a table whose length is part of what is being
  // authored, which no attribute key could address.
  const writeContents = useCollectionContentsWrite();
  const onWriteContents = useCallback(
    (rows: number[][]) =>
      kind && containerId
        ? writeContents(kind, containerId, rows, detail.rows).then(
            refetchDetail,
          )
        : Promise.resolve(),
    [writeContents, refetchDetail, kind, containerId, detail.rows],
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
          {/* Where the Editor reports the state of editing. There is
              one state now: a committed cell is written and saved before
              the field gives focus back, so there is never a count of
              unsaved changes to report and never a Save to press.

              The water-distribution editor used to say "3 unsaved
              changes" here and warm to amber while work was staged. That
              is gone with the staging, and the replacement for it is
              undo rather than discard — which is a promise the editing
              contract makes (§4.5.5) rather than a preference.

              It reads as a state rather than a caption, so it leans on
              the icon and a secondary weight rather than tertiary body
              text. */}
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
          {/* Seven versions of this line described something the editor
              could not do yet, and every one went stale the week after
              it was written — the last of them said links were drawn on
              the map, which stopped being true once a link could name
              its two ends here. Nothing replaced it: the bar says how
              edits are kept, and where an element is added is answered
              by the Add button being there. */}
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
            onReconnect={onReconnect}
            endIds={endIds}
            referenceIds={referenceIds}
            onAdd={
              // The kind can be created and there is a dialog at all —
              // the second clause is the one that was once missing, and
              // the button did nothing when pressed. Every class is
              // placeable here now: a point or a region by a coordinate,
              // a line by its two ends, a container by its name alone.
              CreateNode && creatableHere ? () => setAdding(true) : undefined
            }
            onReveal={spatial ? onReveal : undefined}
            onRename={setRenaming}
            onDelete={setDeleting}
            revealToken={revealToken || undefined}
          />
          {containerId && (
            <CollectionDetail
              detail={detail}
              elementId={containerId}
              onWrite={onWriteContents}
            />
          )}
        </div>
      )}
      {CreateNode && (
        <CreateNode
          open={adding}
          suggestId={suggestId}
          // No gesture behind this one: the dialog asks where to put it,
          // or which two elements to run it between.
          position={null}
          klass={activeClass}
          // The table already said which kind it is showing.
          kind={kind ?? undefined}
          onCreated={(_kind, id) => {
            setAdding(false);
            refetch();
            select(id);
            // A new element goes where the model puts it, which is the
            // end — so on any real network it lands below the fold and
            // the dialog appears to have done nothing. Taking the reader
            // there is honest about where it went; pinning it to the top
            // would not be.
            setRevealId(id);
          }}
          onCancel={() => setAdding(false)}
        />
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
          // Before the delete, not after: everything that follows the
          // selection — the inspector and its result charts — asks the
          // backend about the element it names, and an element that has
          // just stopped existing is one nothing can answer for. The
          // canvas's own delete has always done this; this one did not.
          if (target === selectedId) clearSelection();
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
