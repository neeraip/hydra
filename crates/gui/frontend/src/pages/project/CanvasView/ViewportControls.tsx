import {
  ArrowsPointingOutIcon,
  ArrowsRightLeftIcon,
  MinusIcon,
  PlusIcon,
} from "@heroicons/react/16/solid";
import type { CSSProperties } from "react";

const ICON_14: CSSProperties = { width: 14, height: 14 };

/**
 * Floating zoom / reset-north / fit-network button cluster. Stateless: each
 * button just fires its callback (the caller bumps the matching MapCanvas
 * trigger key).
 *
 * Positioned by its parent rather than by itself, so the strips that stack
 * above it in the same corner need no offset derived from this one's height —
 * see `CanvasView`'s bottom-right column, which also carries the
 * `--inspector-effective-w` offset that keeps the whole stack clear of the
 * element inspector.
 */
export function ViewportControls({
  mapOnly,
  onZoomIn,
  onZoomOut,
  onResetNorth,
  onFit,
}: {
  /** True in schematic mode — reset-north only applies to the map. */
  mapOnly: boolean;
  onZoomIn: () => void;
  onZoomOut: () => void;
  onResetNorth: () => void;
  onFit: () => void;
}) {
  const mapOnlyDim: CSSProperties = {
    opacity: mapOnly ? 0.38 : undefined,
    cursor: mapOnly ? "not-allowed" : undefined,
  };
  return (
    <div
      className="canvas-toolbar"
      style={{
        flexDirection: "column",
        gap: 8,
        // Overrides `.canvas-toolbar`'s uniform 4px: a little more vertical
        // room, matching the aspect slider's box above so the pair reads as one
        // stack. Horizontal padding stays 4px, which is what makes both boxes
        // the same width.
        padding: "8px 4px",
      }}
    >
      <div style={{ display: "flex", flexDirection: "column", gap: 0 }}>
        <button
          type="button"
          className="tool-btn"
          onClick={onZoomIn}
          data-tooltip="Zoom in"
          data-tooltip-pos="left"
          aria-label="Zoom in"
          style={{
            borderBottomLeftRadius: 0,
            borderBottomRightRadius: 0,
          }}
        >
          <PlusIcon style={ICON_14} />
        </button>

        <button
          type="button"
          className="tool-btn"
          onClick={onZoomOut}
          data-tooltip="Zoom out"
          data-tooltip-pos="left"
          aria-label="Zoom out"
          style={{
            borderTopLeftRadius: 0,
            borderTopRightRadius: 0,
            marginTop: -1,
          }}
        >
          <MinusIcon style={ICON_14} />
        </button>
      </div>

      <button
        type="button"
        className="tool-btn"
        onClick={onResetNorth}
        disabled={mapOnly}
        data-tooltip={mapOnly ? "Map mode only" : "Reset north"}
        data-tooltip-pos="left"
        aria-label="Reset north"
        style={mapOnlyDim}
      >
        <ArrowsRightLeftIcon style={ICON_14} />
      </button>

      <button
        type="button"
        className="tool-btn"
        onClick={onFit}
        data-tooltip="Fit network"
        data-tooltip-pos="left"
        aria-label="Fit network"
      >
        <ArrowsPointingOutIcon style={ICON_14} />
      </button>
    </div>
  );
}
