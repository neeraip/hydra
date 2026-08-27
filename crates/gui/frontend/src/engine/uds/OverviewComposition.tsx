import { useLinks, useNodes, useRegions } from "../../hooks";
import { useMeshInfo } from "../../hooks/surface";
import { Kpi, KpiGrid } from "../../pages/project/OverviewView/primitives";

/** Pluralised drainage-kind label for a composition sub-line. */
const KIND_LABELS: Record<string, [string, string]> = {
  junction: ["junction", "junctions"],
  outfall: ["outfall", "outfalls"],
  storage: ["storage unit", "storage units"],
  divider: ["divider", "dividers"],
  conduit: ["conduit", "conduits"],
  pump: ["pump", "pumps"],
  orifice: ["orifice", "orifices"],
  weir: ["weir", "weirs"],
  outlet: ["outlet", "outlets"],
};

/** "12 junctions · 2 outfalls (+1 more)" from a kind→count map. */
function breakdown(counts: Map<string, number>, max = 2): string {
  const entries = [...counts.entries()].sort((a, b) => b[1] - a[1]);
  if (entries.length === 0) return "none";
  const shown = entries
    .slice(0, max)
    .map(([kind, n]) => {
      const [one, many] = KIND_LABELS[kind] ?? [kind, kind];
      return `${n} ${n === 1 ? one : many}`;
    })
    .join(" · ");
  const rest = entries.length - Math.min(entries.length, max);
  return rest > 0 ? `${shown} (+${rest} more)` : shown;
}

function countByKind(items: Array<{ type: string }>): Map<string, number> {
  const counts = new Map<string, number>();
  for (const item of items) {
    counts.set(item.type, (counts.get(item.type) ?? 0) + 1);
  }
  return counts;
}

/**
 * The Overview "Network" KPI grid for urban-drainage projects: counts by
 * the engine's own kinds. Its wds counterpart summarises pipes/tanks/pumps
 * with lengths and diameters; the drainage snapshot carries geometry +
 * identity, so this grid presents exactly what exists and nothing derived.
 */
export function UdsOverviewComposition({
  networkLoaded,
  fallbackNodeCount,
  fallbackLinkCount,
  projectId,
  scenarioId,
}: {
  networkLoaded: boolean;
  fallbackNodeCount: number;
  fallbackLinkCount: number;
  projectId: string;
  scenarioId: string | null;
}) {
  const nodes = useNodes();
  const links = useLinks();
  const regions = useRegions();
  // A 2D surface is a property of the model, so it is stated here from
  // import: the canvas shows one only where the model is placeable, and
  // before this the app said nothing at all about a mesh until a run
  // had produced results to colour it with.
  const mesh = useMeshInfo(projectId, scenarioId, networkLoaded);

  if (!networkLoaded) {
    return (
      <KpiGrid>
        <Kpi
          label="Nodes"
          value={fallbackNodeCount.toLocaleString()}
          sub="loading details…"
          muted
        />
        <Kpi
          label="Links"
          value={fallbackLinkCount.toLocaleString()}
          sub="loading details…"
          muted
        />
        <Kpi label="Subcatchments" value="—" sub="—" muted />
        <Kpi label="Outfalls" value="—" sub="—" muted />
      </KpiGrid>
    );
  }

  const nodeCounts = countByKind(nodes);
  const linkCounts = countByKind(links);
  const outfalls = nodeCounts.get("outfall") ?? 0;
  const withOutlet = regions.filter((r) => r.outletId != null).length;

  return (
    <KpiGrid>
      <Kpi
        label="Nodes"
        value={nodes.length.toLocaleString()}
        sub={breakdown(nodeCounts)}
      />
      <Kpi
        label="Links"
        value={links.length.toLocaleString()}
        sub={breakdown(linkCounts)}
      />
      <Kpi
        label="Subcatchments"
        value={regions.length.toLocaleString()}
        sub={
          regions.length === 0
            ? "no drainage areas mapped"
            : `${withOutlet} with a mapped outlet`
        }
      />
      <Kpi
        label="Outfalls"
        value={outfalls.toLocaleString()}
        sub={outfalls === 0 ? "no discharge points" : "discharge points"}
      />
      {mesh && (
        <Kpi
          label="Surface cells"
          value={mesh.nCells.toLocaleString()}
          sub={`2D overland mesh, ${mesh.nVertices.toLocaleString()} vertices`}
        />
      )}
    </KpiGrid>
  );
}
