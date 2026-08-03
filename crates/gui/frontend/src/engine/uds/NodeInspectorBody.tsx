import { ConnectedLink } from "../../components/panels/ElementInspector/ConnectedElements";
import { SectionLabel } from "../../components/ui/SectionLabel";
import { useLinksConnectedTo } from "../../hooks";
import type { NodeInspectorBodyProps } from "../registry";
import { GenericResultsTable } from "./GenericResultsTable";

/**
 * Urban-drainage node inspector body: current-period results (every §6
 * catalog variable, engine-authored labels and units) + connections. The
 * v4 snapshot carries no attribute data yet — per-element attributes
 * arrive with the §4 attribute serving.
 */
export function UdsNodeInspectorBody({
  node,
  onLocateLink,
  results,
}: NodeInspectorBodyProps) {
  const connectedLinks = useLinksConnectedTo(node.id);
  return (
    <div style={{ flex: 1, overflowY: "auto", padding: 12 }}>
      <GenericResultsTable results={results} />
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
