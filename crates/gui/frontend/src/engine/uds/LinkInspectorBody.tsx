import { ConnectedNodeChip } from "../../components/panels/ElementInspector/ConnectedElements";
import { SectionLabel } from "../../components/ui/SectionLabel";
import { ACCENT, useNodes } from "../../hooks";
import type { LinkInspectorBodyProps } from "../registry";
import {
  GenericResultsCards,
  GenericTimeSeriesCard,
  PropertiesSection,
  useElementDetails,
} from "./inspector-shared";

/**
 * Urban-drainage link inspector body, mirroring the wds body's structure:
 * Properties (the §4 schema's rows with model values), the from/to nodes,
 * and Results as cards (§6 catalog values at the current timeline step).
 */
export function UdsLinkInspectorBody({
  link,
  onLocateNode,
  results,
}: LinkInspectorBodyProps) {
  const allNodes = useNodes();
  const attributes = useElementDetails(link.id, link.type);
  return (
    <div style={{ flex: 1, overflowY: "auto", padding: 12 }}>
      <PropertiesSection {...attributes} />

      <SectionLabel>Connected nodes</SectionLabel>
      <div style={{ display: "flex", gap: 6, marginBottom: 14 }}>
        <ConnectedNodeChip
          label="From"
          nodeId={link.fromId}
          allNodes={allNodes}
          accent={ACCENT}
          onLocate={onLocateNode}
        />
        <ConnectedNodeChip
          label="To"
          nodeId={link.toId}
          allNodes={allNodes}
          accent={ACCENT}
          onLocate={onLocateNode}
        />
      </div>

      <GenericResultsCards results={results} />

      {/* Per-period charts (renders nothing for steady-state runs) */}
      <GenericTimeSeriesCard kind="link" elementId={link.id} />
    </div>
  );
}
