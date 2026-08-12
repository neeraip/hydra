import type { NodeVariable } from "../../../canvas/types";
import type { Node, ResultRanges } from "../../../hooks";
import { useLinksConnectedTo } from "../../../hooks";
import { SectionLabel } from "../../ui/SectionLabel";
import { ConnectedLink } from "./ConnectedElements";
import { PatternPreview } from "./PatternPreview";
import { PropertiesSection, useElementDetails } from "./PropertiesSection";
import { PropRow } from "./primitives";
import { NodeResultsCard } from "./ResultsCards";
import { TimeSeriesCard } from "./TimeSeriesCard";

// ── Node inspector body ────────────────────────────────────────────────────────

export function NodeBody({
  node,
  accent,
  nodeVar,
  ranges,
  hasSimulation,
  isTransitioning,
  onOpenPattern,
  onLocateLink,
}: {
  node: Node;
  accent: string;
  nodeVar?: NodeVariable;
  ranges?: ResultRanges;
  hasSimulation?: boolean;
  isTransitioning?: boolean;
  onOpenPattern?: (id: string) => void;
  onLocateLink: (id: string) => void;
}) {
  const connectedLinks = useLinksConnectedTo(node.id);
  const headPattern = node.headPattern;
  const details = useElementDetails(node.id, node.type);

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
          applies. This body used to write them out itself: hardcoded
          labels, hardcoded units, and read-only, so a junction offered
          every property in the table and none of them here. */}
      <PropertiesSection {...details}>
        {/* The body's own rows, which the schema does not describe: a
            position, and the shape of a referenced pattern. */}
        <PropRow
          label="X / Y"
          value={`${node.x.toFixed(2)}, ${node.y.toFixed(2)}`}
        />
        {headPattern && (
          <tr>
            {/* Spans both columns: the profile needs the full width, and
                it belongs under the pattern's own row rather than as
                another card competing with the results chart below. */}
            <td colSpan={2} style={{ padding: "2px 0 6px" }}>
              <PatternPreview patternId={headPattern} stroke={accent} />
              {onOpenPattern && (
                <button
                  type="button"
                  onClick={() => onOpenPattern(headPattern)}
                  data-tooltip={`Open pattern ${headPattern} in editor`}
                  style={{
                    marginTop: 4,
                    padding: "2px 7px",
                    border: `1px solid ${accent}55`,
                    borderRadius: 4,
                    background: `${accent}14`,
                    color: accent,
                    fontSize: "var(--text-sm)",
                    fontFamily: "var(--font-mono)",
                    cursor: "pointer",
                    fontWeight: 500,
                  }}
                >
                  Open {headPattern}
                </button>
              )}
            </td>
          </tr>
        )}
      </PropertiesSection>

      {/* Connected links */}
      {connectedLinks.length > 0 && (
        <>
          <SectionLabel>
            {connectedLinks.length} connected link
            {connectedLinks.length === 1 ? "" : "s"}
          </SectionLabel>
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 4,
              marginBottom: 14,
            }}
          >
            {connectedLinks.map((l) => (
              <ConnectedLink key={l.id} link={l} onLocate={onLocateLink} />
            ))}
          </div>
        </>
      )}

      {/* Results */}
      <SectionLabel>Results</SectionLabel>
      <NodeResultsCard
        node={node}
        accent={accent}
        nodeVar={nodeVar}
        ranges={ranges}
        hasSimulation={hasSimulation}
      />

      {/* Per-period time series (renders nothing for steady-state runs) */}
      <TimeSeriesCard kind="node" elementId={node.id} />
    </div>
  );
}
