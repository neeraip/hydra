/**
 * The urban-drainage Elements view: one table per element kind.
 *
 * A shared Nodes/Links table can only show columns its kinds have in
 * common, which for drainage is close to nothing — a junction's invert
 * means nothing to an outfall, whose boundary condition means nothing to a
 * storage unit. Giving each kind its own table lets every kind show
 * everything it has, and the tabs come from the engine's own catalog, so
 * this file names no kind and would render an engine it has never heard of.
 */

import { useMemo, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { useCanvasSelection } from "../../canvas/selection-context";
import { KindTable } from "../../components/panels/KindTable";
import { TypeBadge } from "../../components/ui/TypeBadge";
import {
  useElementKinds,
  useKindElements,
  useLinks,
  useNodes,
  useRegions,
} from "../../hooks";

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
  const nodes = useNodes();
  const links = useLinks();
  const regions = useRegions();

  // Only kinds the model actually contains earn a tab: a network with no
  // weirs should not offer a Weirs table to click into and find empty.
  const present = useMemo(() => {
    const counts = new Map<string, number>();
    for (const e of [...nodes, ...links, ...regions]) {
      counts.set(e.type, (counts.get(e.type) ?? 0) + 1);
    }
    return kinds
      .filter((k) => k.class !== "collection" && (counts.get(k.id) ?? 0) > 0)
      .map((k) => ({ ...k, count: counts.get(k.id) ?? 0 }));
  }, [kinds, nodes, links, regions]);

  const [activeKind, setActiveKind] = useState<string | null>(null);
  const kind = present.some((k) => k.id === activeKind)
    ? activeKind
    : (present[0]?.id ?? null);
  const activeClass = present.find((k) => k.id === kind)?.class ?? "point";

  const elements = useKindElements(project?.id, activeScenarioId, kind);

  // The highlight follows whichever selection the visible class owns: a
  // selected conduit must light up its row in the Conduits table, not leave
  // the table looking as though nothing is selected.
  const selectedId =
    activeClass === "point"
      ? selectedNodeId
      : activeClass === "polyline"
        ? selectedLinkId
        : selectedRegionId;

  function select(id: string) {
    if (activeClass === "point") selectNode(id);
    else if (activeClass === "polyline") selectLink(id);
    else selectRegion(id);
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
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        minHeight: 0,
        overflow: "hidden",
      }}
    >
      <div
        style={{
          display: "flex",
          gap: 2,
          padding: "8px 12px 0",
          overflowX: "auto",
          scrollbarWidth: "none",
          flexShrink: 0,
        }}
      >
        {present.map((k) => (
          <button
            type="button"
            key={k.id}
            onClick={() => setActiveKind(k.id)}
            className={`inspector-tab${k.id === kind ? " active" : ""}`}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 6,
              // `.inspector-tab` is `flex: 1` for the two- or three-tab
              // inspector, where filling the width is the point. Here there
              // is one tab per element kind, so stretching them spreads a
              // handful of short labels across the whole page.
              flex: "0 0 auto",
              padding: "8px 10px",
            }}
          >
            <TypeBadge type={k.id} />
            <span>{k.labelPlural}</span>
            <span
              style={{
                fontSize: "var(--text-xs)",
                color: "var(--text-tertiary)",
              }}
            >
              {k.count}
            </span>
          </button>
        ))}
      </div>

      {kind && (
        <KindTable
          kindId={kind}
          elements={elements}
          activeId={selectedId}
          onSelect={select}
        />
      )}
    </div>
  );
}
