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

import { LockClosedIcon } from "@heroicons/react/16/solid";
import { useEffect, useMemo, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { useCanvasSelection } from "../../canvas/selection-context";
import { KindTable } from "../../components/panels/KindTable";
import {
  useCollectionDetail,
  useElementKinds,
  useKindCounts,
  useKindElements,
} from "../../hooks";
import {
  type EditorSection,
  EditorShell,
  EditorStatusBar,
} from "../../pages/project/EditorShell";
import { CollectionDetail } from "./CollectionDetail";

export function UdsElementsView() {
  const { project } = useActiveProject();
  const { activeScenarioId, editorFocus } = useAppState();
  const {
    selectNode,
    selectLink,
    selectRegion,
    selectedNodeId,
    selectedLinkId,
    selectedRegionId,
  } = useCanvasSelection();

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

  const elements = useKindElements(project?.id, activeScenarioId, kind);

  // A container's row reports only its size, so selecting one opens what
  // is actually inside it. A local selection, not the canvas's: a curve
  // has no geometry to highlight.
  const [openContainer, setOpenContainer] = useState<string | null>(null);

  const sections: EditorSection[] = present.map((k) => ({
    id: k.id,
    label: k.labelPlural,
    count: k.count,
    kindId: k.id,
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

  function select(id: string) {
    if (activeClass === "point") selectNode(id);
    else if (activeClass === "polyline") selectLink(id);
    else if (activeClass === "region") selectRegion(id);
    else setOpenContainer(id);
  }

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
              water distribution says "3 unsaved changes" here. Drainage's
              answer is that there is no editing at all, and it is said
              once, in that same place, rather than repeated beside every
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
            <LockClosedIcon
              style={{ width: 12, height: 12, flexShrink: 0 }}
              aria-hidden="true"
            />
            Read-only
          </span>
          {/* "Edited outside Hydra" read as a design stance — as though
              drainage models were deliberately someone else's to change.
              The truth is narrower and temporary: the engine simulates
              drainage fully from the CLI and the SDK, and only the GUI's
              editing has not been built. Say that, so the sentence stops
              being wrong the day it ships rather than merely stale.

              Remove this whole footer when drainage becomes editable;
              `modelEditable` in the engine registry is the switch. */}
          <span style={{ color: "var(--text-tertiary)" }}>
            Editing drainage models here isn't built yet.
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
            revealToken={revealToken || undefined}
          />
          {containerId && (
            <CollectionDetail detail={detail} elementId={containerId} />
          )}
        </div>
      )}
    </EditorShell>
  );
}
