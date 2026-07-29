import { GlobeAltIcon } from "@heroicons/react/16/solid";

/**
 * Centered map-mode alert shown when coordinates can't be reprojected to
 * WGS84. The caller owns the visibility condition (map mode + crsError +
 * not mid-resolution) and the CRS-suggestion sampling; this is presentation
 * only.
 */
export function InvalidCrsOverlay({
  onSetCrs,
}: {
  /** Offer the "Suggest CRS" flow — only meaningful while the CRS is still
   * the EPSG:4326 default (out-of-range coords, not a proj4 failure). */
  onSetCrs: () => void;
}) {
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        pointerEvents: "none",
      }}
    >
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 10,
          padding: "24px 28px",
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          boxShadow: "0 4px 24px rgba(0,0,0,0.18)",
          maxWidth: 360,
          textAlign: "center",
        }}
      >
        <GlobeAltIcon
          style={{ width: 30, height: 30, color: "var(--text-tertiary)" }}
        />
        <span
          style={{
            fontSize: 14,
            fontWeight: 600,
            color: "var(--text-primary)",
            fontFamily: "var(--font-ui)",
          }}
        >
          Invalid coordinate reference system
        </span>
        <span
          style={{
            fontSize: 12,
            color: "var(--text-secondary)",
            fontFamily: "var(--font-ui)",
            lineHeight: 1.6,
          }}
        >
          Map view requires valid WGS84 coordinates. Set the correct source CRS
          to reproject the network, or switch to Schematic view.
        </span>
        <div style={{ display: "flex", gap: 8 }}>
          <button
            type="button"
            className="tool-btn"
            onClick={onSetCrs}
            style={{
              pointerEvents: "auto",
              // .tool-btn is a fixed 30×30 icon button; these are text
              // CTAs, so size to content and give them a border.
              width: "auto",
              border: "1px solid var(--border)",
              padding: "0 12px",
              fontSize: 12,
            }}
          >
            Set source CRS
          </button>
        </div>
      </div>
    </div>
  );
}
