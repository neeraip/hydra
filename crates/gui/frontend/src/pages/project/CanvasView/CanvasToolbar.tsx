import {
  ArrowsRightLeftIcon,
  ChevronUpDownIcon,
  CursorArrowRaysIcon,
  EyeIcon,
  LinkIcon,
  MapPinIcon,
  PencilSquareIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import type { CSSProperties, Dispatch, SetStateAction } from "react";
import type { BasemapStyle } from "../../../canvas/Basemap";
import { useCanvasLayers } from "../../../canvas/layers-context";
import type { CanvasTool, ViewMode } from "../../../canvas/types";
import { CoordStatusIndicator } from "./CoordStatusIndicator";

/** Fixed-size square icon button so toolbar entries align on one grid. */
const ICON_BTN_STYLE: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
};

/** Standard 14px toolbar icon size. */
const ICON_14: CSSProperties = { width: 14, height: 14 };

/** Display label for a basemap style value. */
const basemapLabel = (b: BasemapStyle) =>
  b === "none" ? "No basemap" : b.charAt(0).toUpperCase() + b.slice(1);

/**
 * The canvas' top-left toolbar overlay: view-mode toggle, coordinate-coverage
 * indicator, basemap + CRS pickers, tool buttons, layer visibility toggles,
 * and the scenario-comparison baseline picker.
 *
 * All state stays with the caller (dropdown open flags included — CanvasView
 * owns the global click-outside that closes them via the
 * `data-toolbar-dropdown` markers rendered here).
 */
export function CanvasToolbar({
  viewMode,
  onViewModeChange,
  coordStatus,
  coordMissingCount,
  coordTotalCount,
  basemap,
  onBasemapChange,
  showBasemapDropdown,
  setShowBasemapDropdown,
  sourceCrs,
  crsError,
  onOpenCrsModal,
  activeTool,
  onToolChange,
  hasAnnotations,
  onClearAnnotations,
  showComparePicker,
  comparing,
  baselineName,
  compareOptions,
  effectiveCompareId,
  onSelectCompare,
  showCompareDropdown,
  setShowCompareDropdown,
}: {
  viewMode: ViewMode;
  onViewModeChange: (m: ViewMode) => void;
  coordStatus: "complete" | "partial" | "empty";
  coordMissingCount: number;
  coordTotalCount: number;
  basemap: BasemapStyle;
  onBasemapChange: (b: BasemapStyle) => void;
  showBasemapDropdown: boolean;
  setShowBasemapDropdown: Dispatch<SetStateAction<boolean>>;
  sourceCrs: string;
  crsError: string | null;
  onOpenCrsModal: () => void;
  activeTool: CanvasTool;
  onToolChange: (t: CanvasTool) => void;
  /** True when measure annotations exist (shows the clear button). */
  hasAnnotations: boolean;
  onClearAnnotations: () => void;
  /** Render the comparison picker (same gate as the Legend: results exist). */
  showComparePicker: boolean;
  comparing: boolean;
  baselineName: string;
  compareOptions: { value: string | null; label: string }[];
  effectiveCompareId: string | null;
  onSelectCompare: (value: string | null) => void;
  showCompareDropdown: boolean;
  setShowCompareDropdown: Dispatch<SetStateAction<boolean>>;
}) {
  const { layers: canvasLayers, setLayer } = useCanvasLayers();

  const mapOnly = viewMode !== "map";
  const mapOnlyDim: CSSProperties = {
    opacity: mapOnly ? 0.38 : undefined,
    cursor: mapOnly ? "not-allowed" : undefined,
  };
  const mapOnlyTooltip = (label: string) => (mapOnly ? "Map mode only" : label);

  return (
    <div
      style={{
        position: "absolute",
        top: 12,
        left: "calc(var(--rail-effective-w, 0px) + 12px)",
        zIndex: 10,
        transition: "left var(--rail-transition)",
      }}
    >
      <div className="canvas-toolbar">
        {/* ── VIEW MODE TOGGLE ─────────────────────────────────────────── */}
        <div
          style={{
            display: "flex",
            background: "var(--bg-card)",
            border: "1px solid var(--border)",
            borderRadius: 6,
            padding: 2,
            gap: 2,
            flexShrink: 0,
          }}
        >
          {(["map", "schematic"] as ViewMode[]).map((m) => (
            <button
              type="button"
              key={m}
              onClick={() => onViewModeChange(m)}
              style={{
                border: "none",
                background:
                  viewMode === m ? "var(--accent-dim)" : "transparent",
                color:
                  viewMode === m ? "var(--accent)" : "var(--text-secondary)",
                padding: "3px 10px",
                borderRadius: 4,
                fontSize: 11,
                fontWeight: 600,
                cursor: "pointer",
                fontFamily: "var(--font-ui)",
                letterSpacing: "0.02em",
                whiteSpace: "nowrap",
                flexShrink: 0,
              }}
              data-tooltip={
                m === "map"
                  ? "Geographic layout"
                  : "Idealised orthogonal layout"
              }
              data-tooltip-pos="bottom"
            >
              {m === "map" ? "Map" : "Schematic"}
            </button>
          ))}
        </div>

        {/* Coordinate-coverage indicator — only shown when coords are missing */}
        {viewMode === "map" &&
          coordStatus !== "complete" &&
          coordTotalCount > 0 && (
            <CoordStatusIndicator
              status={coordStatus}
              missingCount={coordMissingCount}
              totalCount={coordTotalCount}
            />
          )}

        {/* Basemap dropdown */}
        <div
          data-toolbar-dropdown
          style={{ position: "relative", opacity: mapOnlyDim.opacity }}
        >
          <button
            type="button"
            className="tool-btn"
            disabled={mapOnly}
            style={{
              width: "auto",
              padding: "0 8px",
              fontSize: 12,
              gap: 4,
              display: "flex",
              alignItems: "center",
              cursor: mapOnlyDim.cursor,
            }}
            onClick={(e) => {
              if (mapOnly) return;
              e.stopPropagation();
              setShowBasemapDropdown((v) => !v);
            }}
            data-tooltip={mapOnlyTooltip("Basemap")}
            data-tooltip-pos="bottom"
          >
            {basemapLabel(basemap)}{" "}
            <ChevronUpDownIcon
              style={{ width: 12, height: 12, verticalAlign: "middle" }}
            />
          </button>
          {showBasemapDropdown && viewMode === "map" && (
            <div
              style={{
                position: "absolute",
                top: "calc(100% + 4px)",
                left: 0,
                background: "var(--bg-panel)",
                border: "1px solid var(--border)",
                borderRadius: 7,
                boxShadow: "var(--shadow-2)",
                overflow: "hidden",
                minWidth: 140,
                zIndex: 20,
              }}
            >
              {(["streets", "light", "dark", "none"] as BasemapStyle[]).map(
                (b) => (
                  <button
                    type="button"
                    key={b}
                    onClick={() => {
                      onBasemapChange(b);
                      setShowBasemapDropdown(false);
                    }}
                    style={{
                      display: "block",
                      width: "100%",
                      padding: "7px 12px",
                      border: "none",
                      background:
                        basemap === b ? "var(--accent-dim)" : "transparent",
                      color:
                        basemap === b
                          ? "var(--accent)"
                          : "var(--text-secondary)",
                      cursor: "pointer",
                      fontSize: 12,
                      textAlign: "left",
                      fontFamily: "var(--font-ui)",
                    }}
                  >
                    {basemapLabel(b)}
                  </button>
                ),
              )}
            </div>
          )}
        </div>

        {/* CRS status + modal launcher */}
        <div
          data-toolbar-dropdown
          style={{ position: "relative", opacity: mapOnlyDim.opacity }}
        >
          <button
            type="button"
            className="tool-btn"
            disabled={mapOnly}
            style={{
              width: "auto",
              padding: "0 8px",
              fontSize: 12,
              gap: 4,
              display: "flex",
              alignItems: "center",
              cursor: mapOnlyDim.cursor,
              borderColor:
                !mapOnly && crsError ? "var(--status-error)" : undefined,
            }}
            onClick={(e) => {
              if (mapOnly) return;
              e.stopPropagation();
              setShowBasemapDropdown(false);
              onOpenCrsModal();
            }}
            data-tooltip={mapOnlyTooltip(
              crsError ?? "Set source coordinate reference system",
            )}
            data-tooltip-pos="bottom"
          >
            {sourceCrs}{" "}
            <ChevronUpDownIcon
              style={{ width: 12, height: 12, verticalAlign: "middle" }}
            />
          </button>
        </div>

        <div className="tool-divider" />

        {/* ── BOTH MODES ───────────────────────────────────────────────── */}

        <button
          type="button"
          className={`tool-btn${activeTool === "select" ? " active" : ""}`}
          onClick={() => onToolChange("select")}
          data-tooltip="Select (S)"
          data-tooltip-pos="bottom"
          aria-label="Select"
          style={ICON_BTN_STYLE}
        >
          <CursorArrowRaysIcon style={ICON_14} />
        </button>

        <button
          type="button"
          className={`tool-btn${activeTool === "edit" ? " active" : ""}`}
          onClick={() => onToolChange("edit")}
          disabled={mapOnly}
          data-tooltip={mapOnlyTooltip("Edit / move nodes (E)")}
          data-tooltip-pos="bottom"
          aria-label="Edit"
          style={{ ...ICON_BTN_STYLE, ...mapOnlyDim }}
        >
          <PencilSquareIcon style={ICON_14} />
        </button>

        <button
          type="button"
          className={`tool-btn${activeTool === "add-node" ? " active" : ""}`}
          disabled={mapOnly}
          onClick={() => onToolChange("add-node")}
          data-tooltip={mapOnlyTooltip("Add node (N)")}
          data-tooltip-pos="bottom"
          aria-label="Add node"
          style={{ ...ICON_BTN_STYLE, ...mapOnlyDim }}
        >
          <MapPinIcon style={ICON_14} />
        </button>

        <button
          type="button"
          className={`tool-btn${activeTool === "add-link" ? " active" : ""}`}
          disabled={mapOnly}
          onClick={() => onToolChange("add-link")}
          data-tooltip={mapOnlyTooltip("Add link (L)")}
          data-tooltip-pos="bottom"
          aria-label="Add link"
          style={{ ...ICON_BTN_STYLE, ...mapOnlyDim }}
        >
          <LinkIcon style={ICON_14} />
        </button>

        {/* Measure distance */}
        <button
          type="button"
          className={`tool-btn${activeTool === "measure" ? " active" : ""}`}
          disabled={mapOnly}
          onClick={() => {
            onToolChange("measure");
            onClearAnnotations();
          }}
          data-tooltip={mapOnlyTooltip("Measure distance (D)")}
          data-tooltip-pos="bottom"
          aria-label="Measure distance"
          style={{
            fontSize: 12,
            fontWeight: 600,
            ...ICON_BTN_STYLE,
            ...mapOnlyDim,
          }}
        >
          <ArrowsRightLeftIcon style={ICON_14} />
        </button>

        {hasAnnotations && viewMode === "map" && (
          <button
            type="button"
            className="tool-btn"
            onClick={onClearAnnotations}
            data-tooltip="Clear annotations"
            data-tooltip-pos="bottom"
            aria-label="Clear annotations"
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              ...ICON_BTN_STYLE,
            }}
          >
            <XMarkIcon style={ICON_14} />
          </button>
        )}

        <div className="tool-divider" />

        {/* Layer visibility toggles */}
        <button
          type="button"
          className={`tool-btn${canvasLayers.model ? " active" : ""}`}
          onClick={() => setLayer("model", !canvasLayers.model)}
          data-tooltip="Toggle base model"
          data-tooltip-pos="bottom"
          aria-label="Toggle base model"
          style={ICON_BTN_STYLE}
        >
          <EyeIcon style={ICON_14} />
        </button>

        <button
          type="button"
          className={`tool-btn${canvasLayers.nodeLabels ? " active" : ""}`}
          onClick={() => setLayer("nodeLabels", !canvasLayers.nodeLabels)}
          data-tooltip="Toggle node labels"
          data-tooltip-pos="bottom"
          style={{ fontSize: 11, fontWeight: 600 }}
        >
          Aa
        </button>

        <button
          type="button"
          className={`tool-btn${canvasLayers.linkLabels ? " active" : ""}`}
          onClick={() => setLayer("linkLabels", !canvasLayers.linkLabels)}
          data-tooltip="Toggle link labels"
          data-tooltip-pos="bottom"
          style={{ fontSize: 11, fontWeight: 600 }}
        >
          Ll
        </button>

        {/* Scenario comparison baseline picker — only when the active
            scenario has results to compare from (same gate as Legend) */}
        {showComparePicker && (
          <>
            <div className="tool-divider" />
            <div data-toolbar-dropdown style={{ position: "relative" }}>
              <button
                type="button"
                className={`tool-btn${comparing ? " active" : ""}`}
                style={{
                  width: "auto",
                  padding: "0 8px",
                  fontSize: 12,
                  gap: 4,
                  display: "flex",
                  alignItems: "center",
                  whiteSpace: "nowrap",
                }}
                onClick={(e) => {
                  e.stopPropagation();
                  setShowBasemapDropdown(false);
                  setShowCompareDropdown((v) => !v);
                }}
                data-tooltip="Colour by difference vs a baseline scenario"
                data-tooltip-pos="bottom"
              >
                {comparing ? `Δ vs ${baselineName}` : "Compare"}{" "}
                <ChevronUpDownIcon
                  style={{ width: 12, height: 12, verticalAlign: "middle" }}
                />
              </button>
              {showCompareDropdown && (
                <div
                  style={{
                    position: "absolute",
                    top: "calc(100% + 4px)",
                    left: 0,
                    background: "var(--bg-panel)",
                    border: "1px solid var(--border)",
                    borderRadius: 7,
                    boxShadow: "var(--shadow-2)",
                    overflow: "hidden auto",
                    minWidth: 140,
                    maxHeight: 280,
                    zIndex: 20,
                  }}
                >
                  {compareOptions.map((o) => (
                    <button
                      type="button"
                      key={o.value ?? "__off__"}
                      onClick={() => {
                        onSelectCompare(o.value);
                        setShowCompareDropdown(false);
                      }}
                      style={{
                        display: "block",
                        width: "100%",
                        padding: "7px 12px",
                        border: "none",
                        background:
                          o.value === effectiveCompareId
                            ? "var(--accent-dim)"
                            : "transparent",
                        color:
                          o.value === effectiveCompareId
                            ? "var(--accent)"
                            : "var(--text-secondary)",
                        cursor: "pointer",
                        fontSize: 12,
                        textAlign: "left",
                        fontFamily: "var(--font-ui)",
                        whiteSpace: "nowrap",
                      }}
                    >
                      {o.label}
                    </button>
                  ))}
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
