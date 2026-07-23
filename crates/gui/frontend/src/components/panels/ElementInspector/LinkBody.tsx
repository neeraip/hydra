import type { LinkVariable } from "../../../canvas/types";
import type { Link, ResultRanges } from "../../../hooks";
import { useNodes } from "../../../hooks";
import { SectionLabel } from "../../ui/SectionLabel";
import { ConnectedNodeChip } from "./ConnectedElements";
import { PropRow } from "./primitives";
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
          <PropRow label="Type" value={link.type} />
          {link.length != null && link.length > 0 && (
            <PropRow label="Length" value={`${link.length.toFixed(1)} m`} />
          )}
          {link.diameter > 0 && (
            <PropRow label="Diameter" value={`${link.diameter} mm`} />
          )}
          {link.roughness != null && link.roughness > 0 && (
            <PropRow label="Roughness" value={String(link.roughness)} />
          )}
          {link.pumpCurve && (
            <PropRow label="Pump curve" value={link.pumpCurve} />
          )}
          {link.pumpPowerKw != null && link.pumpPowerKw > 0 && (
            <PropRow label="Power" value={`${link.pumpPowerKw} kW`} />
          )}
          {link.pumpSpeed != null && link.pumpSpeed > 0 && (
            <PropRow label="Speed" value={`${link.pumpSpeed}`} />
          )}
          {link.valveType && (
            <PropRow label="Valve type" value={link.valveType} />
          )}
          {link.valveSetting != null && (
            <PropRow
              label="Setting"
              value={
                link.valveType === "PRV" ||
                link.valveType === "PSV" ||
                link.valveType === "PBV"
                  ? `${link.valveSetting.toFixed(2)} m`
                  : link.valveType === "FCV"
                    ? `${link.valveSetting.toFixed(3)} L/s`
                    : link.valveType === "TCV"
                      ? `K = ${link.valveSetting.toFixed(3)}`
                      : String(link.valveSetting)
              }
            />
          )}
          {link.valveCurve && <PropRow label="Curve" value={link.valveCurve} />}
        </tbody>
      </table>

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
