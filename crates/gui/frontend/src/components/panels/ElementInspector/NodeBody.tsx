import type { NodeVariable } from "../../../canvas/types";
import type { Node, ResultRanges } from "../../../hooks";
import { useLinksConnectedTo } from "../../../hooks";
import { SectionLabel } from "../../ui/SectionLabel";
import { ConnectedLink } from "./ConnectedElements";
import { PropRow } from "./primitives";
import { NodeResultsCard } from "./ResultsCards";

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
      {/* Static properties */}
      <SectionLabel>Properties</SectionLabel>
      <table
        style={{ width: "100%", borderCollapse: "collapse", marginBottom: 14 }}
      >
        <tbody>
          <PropRow label="Type" value={node.type} />
          {node.elevation != null && (
            <PropRow label="Elevation" value={`${node.elevation} m`} />
          )}
          {node.baseDemand != null && node.baseDemand !== 0 && (
            <PropRow
              label="Base demand"
              value={`${node.baseDemand.toFixed(4)} L/s`}
            />
          )}
          <PropRow
            label="X / Y"
            value={`${node.x.toFixed(2)}, ${node.y.toFixed(2)}`}
          />
          {/* Tank fields */}
          {node.tankMinLevel != null && (
            <PropRow label="Min level" value={`${node.tankMinLevel} m`} />
          )}
          {node.tankMaxLevel != null && (
            <PropRow label="Max level" value={`${node.tankMaxLevel} m`} />
          )}
          {node.tankInitialLevel != null && (
            <PropRow
              label="Initial level"
              value={`${node.tankInitialLevel} m`}
            />
          )}
          {node.tankDiameter != null && node.tankDiameter > 0 && (
            <PropRow label="Tank diameter" value={`${node.tankDiameter} m`} />
          )}
          {node.tankVolumeCurve && (
            <PropRow label="Volume curve" value={node.tankVolumeCurve} />
          )}
          {/* Reservoir fields */}
          {node.headPattern && (
            <tr>
              <td
                style={{
                  fontSize: 12,
                  color: "var(--text-tertiary)",
                  padding: "4px 0",
                  width: "45%",
                }}
              >
                Head pattern
              </td>
              <td style={{ padding: "4px 0" }}>
                <button
                  type="button"
                  onClick={() => onOpenPattern?.(node.headPattern!)}
                  data-tooltip={
                    onOpenPattern
                      ? `Open pattern ${node.headPattern} in editor`
                      : undefined
                  }
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 4,
                    padding: "2px 7px",
                    border: `1px solid ${accent}55`,
                    borderRadius: 4,
                    background: `${accent}14`,
                    color: accent,
                    fontSize: 11,
                    fontFamily: "var(--font-mono)",
                    cursor: onOpenPattern ? "pointer" : "default",
                    fontWeight: 500,
                  }}
                >
                  {node.headPattern}
                </button>
              </td>
            </tr>
          )}
        </tbody>
      </table>

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
    </div>
  );
}
