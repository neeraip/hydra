import type { LinkVariable } from "../../../canvas/types";
import type { Link, ResultRanges } from "../../../hooks";
import { useNodes } from "../../../hooks";
import { SectionLabel } from "../../ui/SectionLabel";
import { ConnectedNodeChip } from "./ConnectedElements";
import { PropertiesSection, useElementDetails } from "./PropertiesSection";
import { LinkResultsCard } from "./ResultsCards";
import { TimeSeriesCard } from "./TimeSeriesCard";

// ── Link inspector body ────────────────────────────────────────────────────────

export function LinkBody({
  link,
  accent,
  linkVar,
  ranges,
  hasSimulation,
  isTransitioning,
  onLocateNode,
}: {
  link: Link;
  accent: string;
  linkVar?: LinkVariable;
  ranges?: ResultRanges;
  hasSimulation?: boolean;
  isTransitioning?: boolean;
  onLocateNode: (id: string) => void;
}) {
  const allNodes = useNodes();
  const details = useElementDetails(link.id, link.type);

  return (
    <div
      style={{
        flex: 1,
        overflowY: "auto",
        padding: 12,
        opacity: isTransitioning ? 0.4 : 1,
        transition: "opacity 220ms ease",
      }}
    >
      {/* Every property the engine declares, editable where it says so
          — the same rows and the same decision the Editor's table
          applies. This body used to write them out itself, with a
          `hasProperties` guard mirroring each row's own condition, a
          hardcoded unit per row, and a nested ternary deciding what a
          valve's setting means. All of it is in the schema. */}
      <PropertiesSection {...details} />

      {/* From / To nodes */}
      <SectionLabel>Connected nodes</SectionLabel>
      <div style={{ display: "flex", gap: 6, marginBottom: 14 }}>
        <ConnectedNodeChip
          label="From"
          nodeId={link.fromId}
          allNodes={allNodes}
          accent={accent}
          onLocate={onLocateNode}
        />
        <ConnectedNodeChip
          label="To"
          nodeId={link.toId}
          allNodes={allNodes}
          accent={accent}
          onLocate={onLocateNode}
        />
      </div>

      {/* Results */}
      <SectionLabel>Results</SectionLabel>
      <LinkResultsCard
        link={link}
        accent={accent}
        linkVar={linkVar}
        ranges={ranges}
        hasSimulation={hasSimulation}
      />

      {/* Per-period time series (renders nothing for steady-state runs) */}
      <TimeSeriesCard kind="link" elementId={link.id} />
    </div>
  );
}
