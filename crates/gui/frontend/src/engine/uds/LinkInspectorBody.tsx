import { ConnectedNodeChip } from "../../components/panels/ElementInspector/ConnectedElements";
import { SectionLabel } from "../../components/ui/SectionLabel";
import type { Link } from "../../hooks";
import { ACCENT, useNodes } from "../../hooks";

/**
 * Urban-drainage link inspector body: endpoints only, for the same reason
 * as the node body — the v4 snapshot is geometry + identity, and the wds
 * property/results vocabulary (diameter, roughness, flow cards) would be
 * fabricated here. See `UdsNodeInspectorBody`.
 */
export function UdsLinkInspectorBody({
  link,
  onLocateNode,
}: {
  link: Link;
  onLocateNode: (id: string) => void;
}) {
  const allNodes = useNodes();
  return (
    <div style={{ flex: 1, overflowY: "auto", padding: 12 }}>
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
