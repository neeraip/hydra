import { useMemo } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { ConnectedNodeChip } from "../../components/panels/ElementInspector/ConnectedElements";
import {
  PropertiesSection,
  useElementDetails,
} from "../../components/panels/ElementInspector/PropertiesSection";
import { SectionLabel } from "../../components/ui/SectionLabel";
import { ACCENT, useInletCouplings, useNodes } from "../../hooks";
import type { LinkInspectorBodyProps } from "../registry";
import { capturedInto } from "./couplings";
import { GenericResultsCards, GenericTimeSeriesCard } from "./inspector-shared";

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
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();
  const allNodes = useNodes();
  const attributes = useElementDetails(link.id, link.type);
  // Where this street's inlets discharge to. Not an endpoint — the sewer
  // node is somewhere below the middle of the street, joined by no pipe —
  // so it is a section of its own rather than a third "connected node".
  const { couplings } = useInletCouplings(project?.id, activeScenarioId);
  const captureNodes = useMemo(
    () => capturedInto(couplings, link.id),
    [couplings, link.id],
  );
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

      {captureNodes.length > 0 && (
        <>
          <SectionLabel>Captures into</SectionLabel>
          <div
            style={{
              display: "flex",
              flexWrap: "wrap",
              gap: 6,
              marginBottom: 14,
            }}
          >
            {captureNodes.map((c) => (
              <ConnectedNodeChip
                key={c.node}
                label={c.design}
                nodeId={c.node}
                allNodes={allNodes}
                accent={ACCENT}
                onLocate={onLocateNode}
              />
            ))}
          </div>
        </>
      )}

      <GenericResultsCards results={results} />

      {/* Per-period charts (renders nothing for steady-state runs) */}
      <GenericTimeSeriesCard kind="link" elementId={link.id} />
    </div>
  );
}
