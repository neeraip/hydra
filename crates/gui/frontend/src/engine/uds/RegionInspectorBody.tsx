import { useHoverActions } from "../../canvas/hover-context";
import { SectionLabel } from "../../components/ui/SectionLabel";
import type { RegionInspectorBodyProps } from "../registry";
import {
  GenericResultsCards,
  PropertiesSection,
  useElementDetails,
} from "./inspector-shared";

/**
 * Urban-drainage subcatchment inspector body, following the same section
 * structure as the node and link bodies: Properties (the §4 schema's rows
 * — rain gage, outlet, area, width, slope, imperviousness), the outlet it
 * discharges to, and Results as cards (rainfall / runoff / infiltration at
 * the current timeline step).
 *
 * No time-series section: the results file's element series are addressed
 * by node/link record, and areal series have no equivalent addressing yet.
 */
export function UdsRegionInspectorBody({
  region,
  onLocateOutlet,
  results,
}: RegionInspectorBodyProps) {
  const attributes = useElementDetails(region.id);
  const { hoverNode, clearHover } = useHoverActions();
  return (
    <div style={{ flex: 1, overflowY: "auto", padding: 12 }}>
      <PropertiesSection rows={attributes} />

      {region.outletId && (
        <>
          <SectionLabel>Discharges to</SectionLabel>
          <div style={{ marginBottom: 14 }}>
            <button
              type="button"
              onClick={() => onLocateOutlet(region.outletId as string)}
              onMouseEnter={() => hoverNode(region.outletId as string)}
              onMouseLeave={() => clearHover()}
              onFocus={() => hoverNode(region.outletId as string)}
              onBlur={() => clearHover()}
              style={{
                background: "var(--bg-card)",
                border: "1px solid var(--border)",
                borderRadius: 6,
                padding: "6px 10px",
                cursor: "pointer",
                fontFamily: "var(--font-mono)",
                fontSize: "var(--text-md)",
                color: "var(--accent)",
              }}
            >
              {region.outletId}
            </button>
          </div>
        </>
      )}

      <GenericResultsCards results={results} />
    </div>
  );
}
