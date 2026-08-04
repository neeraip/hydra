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

import { useEffect, useMemo, useState } from "react";
import { useActiveProject, useAppState, useSimulation } from "../../AppContext";
import { useCurrentPeriod } from "../../canvas/period-context";
import { useCanvasSelection } from "../../canvas/selection-context";
import {
  KindTable,
  type ResultValuesById,
} from "../../components/panels/KindTable";
import { TypeBadge } from "../../components/ui/TypeBadge";
import {
  type DeclaredVariable,
  type DeclaredVariables,
  type ElementClass,
  type GenericVariable,
  getGenericPeriodValues,
  useElementKinds,
  useKindElements,
  useLinks,
  useNodes,
  useRegions,
  useResultVariables,
} from "../../hooks";

/** Result variables belong to a class, so every kind in a class shares
 * them — a junction and an outfall both report depth and flooding.
 *
 * Taken from the engine's declared catalog rather than from a run, so the
 * columns are the same before and after a simulation. A table that grew
 * columns when results appeared changed width under you every time you
 * switched between a simulated and an unsimulated scenario, and said
 * nothing at all about what a run would report. */
function variablesForClass(
  declared: DeclaredVariables,
  cls: ElementClass,
): DeclaredVariable[] {
  if (cls === "point") return declared.pointVars;
  if (cls === "polyline") return declared.polylineVars;
  if (cls === "region") return declared.regionVars;
  return [];
}

/** Where each declared variable sits in a period payload.
 *
 * The payload's arrays are ordered by what the results file actually
 * carries, which need not match the catalog's presentation order — so the
 * join is by id, and a variable the run does not carry simply has no index
 * and renders empty. */
function payloadIndices(
  declared: DeclaredVariable[],
  served: GenericVariable[] | undefined,
): number[] {
  return declared.map((v) => served?.findIndex((s) => s.id === v.id) ?? -1);
}

export function UdsElementsView() {
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();
  const { resultMeta, resultGeneration } = useSimulation();
  const {
    selectNode,
    selectLink,
    selectRegion,
    selectedNodeId,
    selectedLinkId,
    selectedRegionId,
  } = useCanvasSelection();
  const period = useCurrentPeriod();

  const kinds = useElementKinds(project?.engine);
  const declaredVariables = useResultVariables(project?.engine);
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
  const variables = variablesForClass(declaredVariables, activeClass);

  // Current-period results for this class, keyed by element id. The payload
  // is class-wide and snapshot-ordered, so it is joined to ids here.
  const [resultValues, setResultValues] = useState<ResultValuesById>(new Map());
  // biome-ignore lint/correctness/useExhaustiveDependencies: resultGeneration is an intentional refetch trigger after a run.
  useEffect(() => {
    const projectId = project?.id;
    if (!projectId || period == null || variables.length === 0) {
      setResultValues(new Map());
      return;
    }
    let cancelled = false;
    const clear = () => {
      if (!cancelled) setResultValues(new Map());
    };
    const served =
      activeClass === "point"
        ? resultMeta?.generic?.pointVars
        : activeClass === "polyline"
          ? resultMeta?.generic?.polylineVars
          : resultMeta?.generic?.regionVars;
    // No result metadata means this scenario has not been run. Clear rather
    // than ask: the columns stay, holding the same em dash a missing value
    // shows, and the previous scenario's numbers do not sit under the new
    // scenario's name.
    if (!served) {
      clear();
      return () => {
        cancelled = true;
      };
    }
    const indices = payloadIndices(variables, served);
    void getGenericPeriodValues(projectId, period, activeScenarioId)
      .then((payload) => {
        if (cancelled) return;
        if (!payload) {
          clear();
          return;
        }
        const arrays =
          activeClass === "point"
            ? payload.points
            : activeClass === "polyline"
              ? payload.polylines
              : payload.regions;
        // Snapshot order for this class — the same order the canvas uses.
        const ordered =
          activeClass === "point"
            ? nodes
            : activeClass === "polyline"
              ? links
              : regions;
        const next: ResultValuesById = new Map();
        ordered.forEach((el, i) => {
          const row: Record<string, number | null> = {};
          variables.forEach((v, vi) => {
            const at = indices[vi];
            const value = at >= 0 ? arrays[at]?.[i] : undefined;
            row[v.id] = value != null && Number.isFinite(value) ? value : null;
          });
          next.set(el.id, row);
        });
        setResultValues(next);
      })
      // A failed read must not leave the last scenario's numbers on screen
      // either — every path out of here either sets values or clears them.
      .catch(clear);
    return () => {
      cancelled = true;
    };
  }, [
    project?.id,
    activeScenarioId,
    period,
    activeClass,
    resultMeta,
    resultGeneration,
    nodes,
    links,
    regions,
    variables.length,
  ]);

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
          resultVariables={variables}
          resultValues={resultValues}
          activeId={selectedId}
          onSelect={select}
        />
      )}
    </div>
  );
}
