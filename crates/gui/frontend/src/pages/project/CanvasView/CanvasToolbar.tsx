import {
  ArrowsRightLeftIcon,
  ChevronUpDownIcon,
  CursorArrowRaysIcon,
  LinkIcon,
  MapPinIcon,
  PencilSquareIcon,
} from "@heroicons/react/16/solid";
import type { CSSProperties, Dispatch, SetStateAction } from "react";
import {
  type BasemapId,
  basemapDisplayLabel,
  basemapPickerGroups,
  clampBasemapOpacity,
} from "../../../canvas/Basemap";
import { LOCAL_CRS } from "../../../canvas/coords";
import { useCanvasLayers } from "../../../canvas/layers-context";
import type { MeasurePoint } from "../../../canvas/measureSnap";
import type { CanvasTool, ViewMode } from "../../../canvas/types";
import { useRegions } from "../../../hooks";
import {
  useBasemapProviders,
  useBasemapVisibility,
} from "../../../hooks/basemapProviders";
import { CoordStatusIndicator } from "./CoordStatusIndicator";
import { MeasurePopover } from "./MeasurePopover";

/** Fixed-size square icon button so toolbar entries align on one grid. */
const ICON_BTN_STYLE: CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
};

/** Standard 14px toolbar icon size. */
const ICON_14: CSSProperties = { width: 14, height: 14 };

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
  editable,
  viewMode,
  localGrid = false,
  onViewModeChange,
  coordStatus,
  coordMissingCount,
  coordTotalCount,
  basemap,
  onBasemapChange,
  basemapOpacity,
  onBasemapOpacityChange,
  showBasemapDropdown,
  setShowBasemapDropdown,
  sourceCrs,
  crsError,
  onOpenCrsModal,
  onOpenBasemapProviders,
  activeTool,
  onToolChange,
  measurePoints,
  measureDistanceM,
  onClearAnnotations,
}: {
  /** Whether model-editing tools (edit, add node, add link) appear. */
  editable: boolean;
  viewMode: ViewMode;
  /** The model has no georeference: there is no map to switch to, and no
   * basemap to draw under it. */
  localGrid?: boolean;
  onViewModeChange: (m: ViewMode) => void;
  coordStatus: "complete" | "partial" | "empty";
  coordMissingCount: number;
  coordTotalCount: number;
  basemap: BasemapId;
  onBasemapChange: (b: BasemapId) => void;
  /** Basemap dimming, 0–1 (1 = fully opaque). */
  basemapOpacity: number;
  onBasemapOpacityChange: (v: number) => void;
  showBasemapDropdown: boolean;
  setShowBasemapDropdown: Dispatch<SetStateAction<boolean>>;
  sourceCrs: string;
  crsError: string | null;
  onOpenCrsModal: () => void;
  /** Opens the basemap-providers management modal ("Manage basemaps…"). */
  onOpenBasemapProviders: () => void;
  activeTool: CanvasTool;
  onToolChange: (t: CanvasTool) => void;
  /** Committed measure points — drives the readout under the measure button. */
  measurePoints: readonly MeasurePoint[];
  /** Measured distance in metres, once two points exist. */
  measureDistanceM: number | null;
  onClearAnnotations: () => void;
}) {
  const { layers: canvasLayers, setLayer } = useCanvasLayers();
  // Region polygons exist only for models that drew any — the toggle
  // follows the data, not the engine.
  const hasRegions = useRegions().length > 0;
  const basemapProviders = useBasemapProviders();
  const basemapVisibility = useBasemapVisibility();

  // Grouped picker entries: unlabeled OpenFreeMap group first, then one group
  // per connected provider; hidden styles filtered out (provider styles are
  // hidden by default until explicitly unhidden in the providers modal).
  const pickerGroups = basemapPickerGroups(basemapProviders, basemapVisibility);
  const anyVisibleStyles = pickerGroups.length > 0;
  // Hiding never changes the active map: a hidden-but-active style stays in
  // the picker, marked "(hidden)", until the user picks something else.
  const activeIsListed =
    basemap === "none" ||
    pickerGroups.some((g) => g.entries.some((e) => e.id === basemap));

  /** One selectable style row in the basemap dropdown. */
  function basemapEntry(id: BasemapId, label: string) {
    return (
      <button
        type="button"
        key={id}
        onClick={() => {
          onBasemapChange(id);
          setShowBasemapDropdown(false);
        }}
        style={{
          display: "block",
          width: "100%",
          padding: "7px 12px",
          border: "none",
          background: basemap === id ? "var(--accent-dim)" : "transparent",
          color: basemap === id ? "var(--accent)" : "var(--text-secondary)",
          cursor: "pointer",
          fontSize: "var(--text-md)",
          textAlign: "left",
          fontFamily: "var(--font-ui)",
        }}
      >
        {label}
      </button>
    );
  }

  // A local grid never enters map mode, so the map-only affordances stay
  // hidden rather than offering a basemap for coordinates no basemap can
  // place.
  const mapOnly = localGrid || viewMode !== "map";
  const mapOnlyDim: CSSProperties = {
    opacity: mapOnly ? 0.38 : undefined,
    cursor: mapOnly ? "not-allowed" : undefined,
  };
  // A local grid is disabled for a different reason than the schematic is:
  // it has real coordinates, it simply has no georeference, so saying "map
  // mode only" to someone already looking at their plan reads as a bug.
  const mapOnlyTooltip = (label: string) =>
    !mapOnly
      ? label
      : localGrid
        ? "Needs a georeferenced model"
        : "Map mode only";

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
          {/* Both views exist for every project. What differs is what the
              first one *is*: a georeferenced model gets a map, a local grid
              gets its plan — the model's own coordinates with no basemap to
              put them on. Both are real positions, which is what separates
              them from the schematic's invented ones. */}
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
                fontSize: "var(--text-sm)",
                fontWeight: 600,
                cursor: "pointer",
                fontFamily: "var(--font-ui)",
                letterSpacing: "0.02em",
                whiteSpace: "nowrap",
                flexShrink: 0,
              }}
              data-tooltip={
                m === "schematic"
                  ? "Idealised orthogonal layout"
                  : localGrid
                    ? "The model's own coordinates — its CRS is a local grid"
                    : "Geographic layout"
              }
              data-tooltip-pos="bottom"
            >
              {m === "schematic" ? "Schematic" : localGrid ? "Plan" : "Map"}
            </button>
          ))}
        </div>

        {/* Coordinate-coverage indicator — only shown when coords are missing */}
        {!localGrid &&
          viewMode === "map" &&
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
              fontSize: "var(--text-md)",
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
            {basemapDisplayLabel(basemap, basemapProviders)}{" "}
            <ChevronUpDownIcon
              style={{ width: 12, height: 12, verticalAlign: "middle" }}
            />
          </button>
          {showBasemapDropdown && viewMode === "map" && !localGrid && (
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
                minWidth: 160,
                zIndex: 20,
              }}
            >
              {basemapEntry("none", "No basemap")}
              {!activeIsListed &&
                basemapEntry(
                  basemap,
                  `${basemapDisplayLabel(basemap, basemapProviders)} (hidden)`,
                )}
              {pickerGroups.map((g) => (
                <div key={g.providerId}>
                  <div
                    style={{
                      padding: "6px 12px 2px",
                      fontSize: "var(--text-xs)",
                      fontWeight: 600,
                      letterSpacing: "0.06em",
                      textTransform: "uppercase",
                      color: "var(--text-tertiary)",
                      fontFamily: "var(--font-ui)",
                    }}
                  >
                    {g.label}
                  </div>
                  {g.entries.map((e) => basemapEntry(e.id, e.label))}
                </div>
              ))}
              {!anyVisibleStyles && (
                <div
                  style={{
                    padding: "7px 12px",
                    fontSize: "var(--text-sm)",
                    color: "var(--text-tertiary)",
                    fontFamily: "var(--font-ui)",
                  }}
                >
                  All styles hidden — use Manage basemaps…
                </div>
              )}
              {/* Opacity slider — dims the basemap live while the dropdown
                  stays open (it sits inside the data-toolbar-dropdown
                  container, so the global click-outside never fires). */}
              <div
                style={{
                  borderTop: "1px solid var(--border)",
                  marginTop: 2,
                  padding: "7px 12px 8px",
                  display: "flex",
                  alignItems: "center",
                  gap: 8,
                }}
              >
                <span
                  style={{
                    fontSize: "var(--text-xs)",
                    fontWeight: 600,
                    letterSpacing: "0.06em",
                    textTransform: "uppercase",
                    color: "var(--text-tertiary)",
                    fontFamily: "var(--font-ui)",
                    whiteSpace: "nowrap",
                  }}
                >
                  Opacity
                </span>
                <input
                  type="range"
                  min={0}
                  max={100}
                  step={1}
                  value={Math.round(basemapOpacity * 100)}
                  onChange={(e) =>
                    onBasemapOpacityChange(
                      clampBasemapOpacity(Number(e.currentTarget.value) / 100),
                    )
                  }
                  aria-label="Basemap opacity"
                  style={{
                    flex: 1,
                    minWidth: 80,
                    accentColor: "var(--accent)",
                  }}
                />
                <span
                  style={{
                    fontSize: "var(--text-xs)",
                    color: "var(--text-secondary)",
                    fontFamily: "var(--font-mono)",
                    fontVariantNumeric: "tabular-nums",
                    width: 30,
                    textAlign: "right",
                    flexShrink: 0,
                  }}
                >
                  {Math.round(basemapOpacity * 100)}%
                </span>
              </div>
              <div style={{ borderTop: "1px solid var(--border)" }} />
              <button
                type="button"
                onClick={() => {
                  setShowBasemapDropdown(false);
                  onOpenBasemapProviders();
                }}
                style={{
                  display: "block",
                  width: "100%",
                  padding: "7px 12px",
                  border: "none",
                  background: "transparent",
                  color: "var(--text-secondary)",
                  cursor: "pointer",
                  fontSize: "var(--text-md)",
                  textAlign: "left",
                  fontFamily: "var(--font-ui)",
                }}
              >
                Manage basemaps…
              </button>
            </div>
          )}
        </div>

        {/* CRS status + modal launcher.
            Never gated on the view: which coordinate system a model is in
            is a property of the model, not of how it is being drawn — and
            gating it on `localGrid` was a trap, since this is the control
            you would use to say a model is *not* on a local grid. */}
        <div data-toolbar-dropdown style={{ position: "relative" }}>
          <button
            type="button"
            className="tool-btn"
            style={{
              width: "auto",
              padding: "0 8px",
              fontSize: "var(--text-md)",
              gap: 4,
              display: "flex",
              alignItems: "center",
              borderColor: crsError ? "var(--status-error)" : undefined,
            }}
            onClick={(e) => {
              e.stopPropagation();
              setShowBasemapDropdown(false);
              onOpenCrsModal();
            }}
            data-tooltip={crsError ?? "Set source coordinate reference system"}
            data-tooltip-pos="bottom"
          >
            {sourceCrs === LOCAL_CRS ? "Local grid" : sourceCrs}{" "}
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

        {editable && (
          <>
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

            {/* Not map-only, unlike its neighbours: a link carries no coordinates of
            its own — `create_link` takes two node ids — so the schematic's
            synthetic positions are irrelevant to it. Connecting nodes is often
            easier there, where the layout makes connectivity legible. */}
            <button
              type="button"
              className={`tool-btn${activeTool === "add-link" ? " active" : ""}`}
              onClick={() => onToolChange("add-link")}
              data-tooltip="Add link (L)"
              data-tooltip-pos="bottom"
              aria-label="Add link"
              style={ICON_BTN_STYLE}
            >
              <LinkIcon style={ICON_14} />
            </button>
          </>
        )}

        {/* Measure distance, with its readout anchored underneath */}
        <div style={{ position: "relative", display: "inline-flex" }}>
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
              fontSize: "var(--text-md)",
              fontWeight: 600,
              ...ICON_BTN_STYLE,
              ...mapOnlyDim,
            }}
          >
            <ArrowsRightLeftIcon style={ICON_14} />
          </button>
          {activeTool === "measure" && (
            <MeasurePopover
              points={measurePoints}
              distanceM={measureDistanceM}
              onExit={() => {
                onClearAnnotations();
                onToolChange("select");
              }}
            />
          )}
        </div>

        <div className="tool-divider" />

        {/* Layer visibility toggles.
            Nodes and links toggle independently: one "base model" switch could
            only turn the network off wholesale, and the useful move is dropping
            one so the other is legible — links in a dense area, or nodes when
            tracing connectivity. The glyphs match the inspector's convention, a
            dot for a node and a bar for a link. */}
        <button
          type="button"
          className={`tool-btn${canvasLayers.nodes ? " active" : ""}`}
          onClick={() => setLayer("nodes", !canvasLayers.nodes)}
          data-tooltip="Toggle nodes"
          data-tooltip-pos="bottom"
          aria-label="Toggle nodes"
          style={ICON_BTN_STYLE}
        >
          <span
            aria-hidden
            style={{
              width: 9,
              height: 9,
              borderRadius: "50%",
              background: "currentColor",
            }}
          />
        </button>

        <button
          type="button"
          className={`tool-btn${canvasLayers.links ? " active" : ""}`}
          onClick={() => setLayer("links", !canvasLayers.links)}
          data-tooltip="Toggle links"
          data-tooltip-pos="bottom"
          aria-label="Toggle links"
          style={ICON_BTN_STYLE}
        >
          <span
            aria-hidden
            style={{
              width: 13,
              height: 2.5,
              borderRadius: 2,
              background: "currentColor",
            }}
          />
        </button>

        {hasRegions && (
          <button
            type="button"
            className={`tool-btn${canvasLayers.regions ? " active" : ""}`}
            onClick={() => setLayer("regions", !canvasLayers.regions)}
            data-tooltip="Toggle subcatchment areas (map view only)"
            data-tooltip-pos="bottom"
            aria-label="Toggle regions"
            style={ICON_BTN_STYLE}
          >
            <span
              aria-hidden
              style={{
                width: 11,
                height: 9,
                borderRadius: 2,
                border: "1.5px solid currentColor",
                transform: "skewX(-8deg)",
              }}
            />
          </button>
        )}

        <button
          type="button"
          className={`tool-btn${canvasLayers.nodeLabels ? " active" : ""}`}
          onClick={() => setLayer("nodeLabels", !canvasLayers.nodeLabels)}
          data-tooltip="Toggle node labels"
          data-tooltip-pos="bottom"
          style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}
        >
          Aa
        </button>

        <button
          type="button"
          className={`tool-btn${canvasLayers.linkLabels ? " active" : ""}`}
          onClick={() => setLayer("linkLabels", !canvasLayers.linkLabels)}
          data-tooltip="Toggle link labels"
          data-tooltip-pos="bottom"
          style={{ fontSize: "var(--text-sm)", fontWeight: 600 }}
        >
          Ll
        </button>
      </div>
    </div>
  );
}
