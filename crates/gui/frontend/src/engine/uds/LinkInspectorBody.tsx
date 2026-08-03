import { ConnectedNodeChip } from "../../components/panels/ElementInspector/ConnectedElements";
import { SectionLabel } from "../../components/ui/SectionLabel";
import { ACCENT, useNodes } from "../../hooks";
import type { LinkInspectorBodyProps } from "../registry";
import { GenericResultsTable } from "./GenericResultsTable";

/**
 * Urban-drainage link inspector body: current-period results + endpoints.
 * The v4 snapshot is geometry + identity, so the wds property vocabulary
 * (diameter, roughness) would be fabricated here — see
 * `UdsNodeInspectorBody`.
 */
export function UdsLinkInspectorBody({
  link,
  onLocateNode,
  results,
}: LinkInspectorBodyProps) {
  const allNodes = useNodes();
  return (
    <div style={{ flex: 1, overflowY: "auto", padding: 12 }}>
      <GenericResultsTable results={results} />
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
    </div>
  );
}
