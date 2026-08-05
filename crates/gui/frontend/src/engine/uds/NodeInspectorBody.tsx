import { useMemo } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { useHoverActions } from "../../canvas/hover-context";
import { ConnectedLink } from "../../components/panels/ElementInspector/ConnectedElements";
import { SectionLabel } from "../../components/ui/SectionLabel";
import {
  useInletCouplings,
  useLinks,
  useLinksConnectedTo,
  useRegions,
} from "../../hooks";
import type { NodeInspectorBodyProps } from "../registry";
import { capturedFrom } from "./couplings";
import {
  GenericResultsCards,
  GenericTimeSeriesCard,
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
  onLocateRegion,
  results,
}: NodeInspectorBodyProps) {
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();
  const connectedLinks = useLinksConnectedTo(node.id);
  const attributes = useElementDetails(node.id, node.type);
  const { hoverRegion, clearHover } = useHoverActions();
  // The reverse of a catchment's "Discharges to". A catchment reaches its
  // outlet through no link at all, so this relationship is invisible from
  // the node's side unless it is looked up — and from the node's side is
  // exactly where you ask what is loading it.
  const regions = useRegions();
  const draining = useMemo(
    () => regions.filter((r) => r.outletId === node.id),
    [regions, node.id],
  );

  // Street conduits that discharge into this node through an inlet. Kept
  // apart from "connected links" rather than folded in: those share an
  // endpoint with this node, these do not touch it at all — which is the
  // distinction the map draws as a dashed line instead of a pipe.
  const { couplings } = useInletCouplings(project?.id, activeScenarioId);
  const allLinks = useLinks();
  const capturingLinks = useMemo(() => {
    const ids = new Set(capturedFrom(couplings, node.id));
    return allLinks.filter((l) => ids.has(l.id));
  }, [couplings, allLinks, node.id]);
  return (
    <div style={{ flex: 1, overflowY: "auto", padding: 12 }}>
      <PropertiesSection {...attributes} />

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

      {capturingLinks.length > 0 && (
        <>
          <SectionLabel>
            Captured from {capturingLinks.length} conduit
            {capturingLinks.length === 1 ? "" : "s"}
          </SectionLabel>
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: 4,
              marginBottom: 14,
            }}
          >
            {capturingLinks.map((l) => (
              <ConnectedLink key={l.id} link={l} onLocate={onLocateLink} />
            ))}
          </div>
        </>
      )}

      {draining.length > 0 && (
        <>
          <SectionLabel>
            {draining.length} catchment{draining.length === 1 ? "" : "s"} drain
            {draining.length === 1 ? "s" : ""} here
          </SectionLabel>
          <div
            style={{
              display: "flex",
              flexWrap: "wrap",
              gap: 4,
              marginBottom: 14,
            }}
          >
            {draining.map((r) => (
              <button
                type="button"
                key={r.id}
                onClick={() => onLocateRegion?.(r.id)}
                onMouseEnter={() => hoverRegion(r.id)}
                onMouseLeave={() => clearHover()}
                onFocus={() => hoverRegion(r.id)}
                onBlur={() => clearHover()}
                disabled={!onLocateRegion}
                style={{
                  background: "var(--bg-card)",
                  border: "1px solid var(--border)",
                  borderRadius: 6,
                  padding: "6px 10px",
                  cursor: onLocateRegion ? "pointer" : "default",
                  fontFamily: "var(--font-mono)",
                  fontSize: "var(--text-md)",
                  color: "var(--accent)",
                }}
              >
                {r.id}
              </button>
            ))}
          </div>
        </>
      )}

      <GenericResultsCards results={results} />

      {/* Per-period charts (renders nothing for steady-state runs) */}
      <GenericTimeSeriesCard kind="node" elementId={node.id} />
    </div>
  );
}
