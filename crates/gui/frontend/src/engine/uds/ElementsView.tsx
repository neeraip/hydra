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

import { useMemo, useState } from "react";
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
import { engineComponents } from "../registry";
import { CollectionDetail } from "./CollectionDetail";

export function UdsElementsView() {
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();
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
  const { modelEditable } = engineComponents(project?.engine);

  // Which kinds earn a rail entry depends on whether the model can be
  // edited, not on which engine it is.
  //
  // In an editable model an empty kind is where the first weir gets
  // added, so it has to be reachable even at zero. In a read-only one it
  // is a table that can be neither read nor filled, so listing it offers
  // a click that leads nowhere. The water-distribution and drainage
  // editors already behaved this way; deriving it from the capability
  // makes that a rule rather than a coincidence.
  //
  // Collections are included like any other kind — they are the model's
  // pollutants, curves, time series, patterns and rules.
  const present = useMemo(
    () =>
      kinds
        .map((k) => ({ ...k, count: counts[k.id] ?? 0 }))
        .filter((k) => modelEditable || k.count > 0),
    [kinds, counts, modelEditable],
  );

  const [activeKind, setActiveKind] = useState<string | null>(null);
  const kind = present.some((k) => k.id === activeKind)
    ? activeKind
    : (present[0]?.id ?? null);
  const activeClass = present.find((k) => k.id === kind)?.class ?? "point";

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

  if (present.length === 0) {
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
          {/* The status bar says why it is empty rather than leaving a bar
              that looks like something failed to load. Read-only engines
              hide edit affordances instead of offering ones that refuse. */}
          <span style={{ color: "var(--text-tertiary)" }}>
            Read-only — drainage models are edited outside Hydra
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
          />
          {containerId && (
            <CollectionDetail detail={detail} elementId={containerId} />
          )}
        </div>
      )}
    </EditorShell>
  );
}
