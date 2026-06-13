import type React from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { type ProjectView, useAppState } from "../../AppContext";
import { useCanvasSelection } from "../../canvas/selection-context";
import { NetworkInspectorHome } from "../panels/NetworkInspectorHome";

const RAIL_MIN = 200;
const RAIL_MAX = 520;
const RAIL_DEFAULT = 280;
const STORAGE_KEY = "hydra2-rail-width";

// ── Rail content per view ─────────────────────────────────────────────────────

function CanvasRail() {
  const {
    selectNode,
    selectLink,
    selectedNodeId,
    selectedLinkId,
    simNodes,
    simLinks,
    zoomToNode,
    zoomToLink,
  } = useCanvasSelection();

  return (
    <NetworkInspectorHome
      embedded
      onSelectNode={selectNode}
      onSelectLink={selectLink}
      activeNodeId={selectedNodeId}
      activeLinkId={selectedLinkId}
      nodes={simNodes ?? undefined}
      links={simLinks ?? undefined}
      onZoomToNode={zoomToNode}
      onZoomToLink={zoomToLink}
    />
  );
}

// ── Root component ────────────────────────────────────────────────────────────

// Rails are defined only for views with meaningful side-panel content.
// Views without an entry here (overview) collapse the rail entirely
// so the body fills the window.
const RAIL_CONTENT: Partial<Record<ProjectView, React.ComponentType>> = {
  canvas: CanvasRail,
};

export function SecondaryRail() {
  const { page, projectView, railOpen, activeProjectId, toggleRail } =
    useAppState();

  const [railWidth, setRailWidth] = useState<number>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    const parsed = stored ? parseInt(stored, 10) : NaN;
    return Number.isFinite(parsed)
      ? Math.min(RAIL_MAX, Math.max(RAIL_MIN, parsed))
      : RAIL_DEFAULT;
  });

  // Refs for direct DOM manipulation during drag — avoids React re-renders.
  const outerRef = useRef<HTMLDivElement>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const railWidthRef = useRef(railWidth);
  railWidthRef.current = railWidth;

  const isDragging = useRef(false);
  const dragStartX = useRef(0);
  const dragStartWidth = useRef(0);

  // Apply width imperatively to DOM elements, bypassing React reconciliation.
  const applyWidth = useCallback((w: number, open: boolean) => {
    const px = `${w}px`;
    if (outerRef.current) outerRef.current.style.width = open ? px : "0px";
    document.documentElement.style.setProperty(
      "--rail-effective-w",
      open ? px : "0px",
    );
  }, []);

  const handleResizeMouseDown = useCallback(
    (e: React.MouseEvent) => {
      e.preventDefault();
      isDragging.current = true;
      dragStartX.current = e.clientX;
      dragStartWidth.current = railWidthRef.current;
      document.body.style.cursor = "col-resize";
      document.body.style.userSelect = "none";

      // Disable the CSS transition on the outer div during drag for zero-lag tracking.
      if (outerRef.current) outerRef.current.style.transition = "none";

      function onMouseMove(ev: MouseEvent) {
        if (!isDragging.current) return;
        const delta = ev.clientX - dragStartX.current;
        const next = Math.min(
          RAIL_MAX,
          Math.max(RAIL_MIN, dragStartWidth.current + delta),
        );
        applyWidth(next, true);
        railWidthRef.current = next;
      }

      function onMouseUp() {
        isDragging.current = false;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        // Re-enable transition before committing to state.
        if (outerRef.current) outerRef.current.style.transition = "";
        const finalWidth = railWidthRef.current;
        localStorage.setItem(STORAGE_KEY, String(finalWidth));
        setRailWidth(finalWidth);
        document.removeEventListener("mousemove", onMouseMove);
        document.removeEventListener("mouseup", onMouseUp);
      }

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
    },
    [applyWidth],
  );

  // Keep --rail-effective-w in sync for non-drag changes (open/close, initial mount).
  // Must be before any early returns to satisfy the Rules of Hooks.
  useEffect(() => {
    const active =
      page === "project" && !!activeProjectId && !!RAIL_CONTENT[projectView];
    document.documentElement.style.setProperty(
      "--rail-effective-w",
      active && railOpen ? `${railWidth}px` : "0px",
    );
    return () => {
      document.documentElement.style.setProperty("--rail-effective-w", "0px");
    };
  }, [page, activeProjectId, projectView, railOpen, railWidth]);

  if (page !== "project" || !activeProjectId) return null;

  const Content = RAIL_CONTENT[projectView];
  if (!Content) return null;

  const w = `${railWidth}px`;

  return (
    /* Floats over the canvas — position:absolute removes it from flex flow
       so the map always fills the full container width. */
    <div
      ref={outerRef}
      style={{
        position: "absolute",
        left: 0,
        top: 0,
        bottom: "var(--timeline-h, 0px)",
        width: railOpen ? w : "0px",
        zIndex: 20,
        transition: "width var(--rail-transition)",
        willChange: "width",
      }}
    >
      {/* Clip layer: inset-0 so it follows the animated width exactly.
          overflow:hidden here clips the content without affecting the toggle. */}
      <div
        style={{
          position: "absolute",
          inset: 0,
          overflow: "hidden",
          background: "var(--bg-rail)",
          borderRight: "1px solid var(--border)",
        }}
      >
        {/* Content fills the clip layer — width tracked via DOM during drag */}
        <div
          ref={contentRef}
          style={{
            width: "100%",
            height: "100%",
            display: "flex",
            flexDirection: "column",
            overflow: "hidden",
          }}
        >
          <Content />
        </div>

        {/* Resize handle — sits on the right edge of the clip layer */}
        {/* biome-ignore lint/a11y/noStaticElementInteractions: resize handle is pointer-driven only. */}
        <div
          onMouseDown={handleResizeMouseDown}
          style={{
            position: "absolute",
            top: 0,
            right: 0,
            width: 5,
            height: "100%",
            cursor: "col-resize",
            zIndex: 10,
          }}
        />
      </div>

      {/* Toggle tab — sibling of the clip layer so translateX(100%) is never clipped */}
      <button
        type="button"
        onClick={toggleRail}
        data-tooltip={railOpen ? "Collapse panel" : "Expand panel"}
        style={{
          position: "absolute",
          right: 0,
          top: "50%",
          transform: "translateX(100%) translateY(-50%)",
          zIndex: 20,
          width: 14,
          height: 44,
          border: "1px solid var(--border)",
          borderLeft: "none",
          background: "var(--bg-rail)",
          color: "var(--text-tertiary)",
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          borderRadius: "0 5px 5px 0",
          padding: 0,
          fontSize: 10,
          lineHeight: 1,
          transition: "color var(--t-fast), background var(--t-fast)",
        }}
        onMouseEnter={(e) => {
          const el = e.currentTarget as HTMLButtonElement;
          el.style.color = "var(--text-primary)";
          el.style.background = "var(--bg-card)";
        }}
        onMouseLeave={(e) => {
          const el = e.currentTarget as HTMLButtonElement;
          el.style.color = "var(--text-tertiary)";
          el.style.background = "var(--bg-rail)";
        }}
      >
        {railOpen ? "‹" : "›"}
      </button>
    </div>
  );
}
