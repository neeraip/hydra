import { ConnectedLink } from "../../components/panels/ElementInspector/ConnectedElements";
import { SectionLabel } from "../../components/ui/SectionLabel";
import { useLinksConnectedTo } from "../../hooks";
import type { NodeInspectorBodyProps } from "../registry";
import {
  GenericResultsCards,
  PropertiesSection,
  useElementDetails,
} from "./inspector-shared";

/**
 * Urban-drainage node inspector body, mirroring the wds body's structure:
 * Properties (the §4 schema's rows with model values), connected links,
 * and Results as cards (§6 catalog values at the current timeline step).
 */
export function UdsNodeInspectorBody({
  node,
  onLocateLink,
  results,
}: NodeInspectorBodyProps) {
  const connectedLinks = useLinksConnectedTo(node.id);
  const attributes = useElementDetails(node.id);
  return (
    <div style={{ flex: 1, overflowY: "auto", padding: 12 }}>
      <PropertiesSection rows={attributes} />

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

      <GenericResultsCards results={results} />
    </div>
  );
}
