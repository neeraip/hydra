import { ConnectedLink } from "../../components/panels/ElementInspector/ConnectedElements";
import { SectionLabel } from "../../components/ui/SectionLabel";
import type { Node } from "../../hooks";
import { useLinksConnectedTo } from "../../hooks";

/**
 * Urban-drainage node inspector body: identity + connections only. The v4
 * snapshot carries no attribute data yet, and the wds body's vocabulary
 * (elevation, base demand, pressure cards) is wrong for drainage nodes —
 * per-element attributes and results arrive with the §4 attribute serving.
 * Current-period values already show on hover and through the canvas
 * colouring.
 */
export function UdsNodeInspectorBody({
  node,
  onLocateLink,
}: {
  node: Node;
  onLocateLink: (id: string) => void;
}) {
  const connectedLinks = useLinksConnectedTo(node.id);
  return (
    <div style={{ flex: 1, overflowY: "auto", padding: 12 }}>
      {connectedLinks.length > 0 ? (
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
      ) : (
        <div
          style={{
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
          }}
        >
          No connected links.
        </div>
      )}
    </div>
  );
}
