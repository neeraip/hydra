/**
 * ElementInspector — unified inspector panel for a selected node or link.
 *
 * Replaces the tabbed Inspector + LinkInspector pair with a single scrollable
 * view that shows all available data. Sections that have no data (e.g. sim
 * results before a run) are omitted entirely rather than showing placeholders.
 *
 * Layout
 * ──────
 * • Header     — element id, type badge, back / close buttons
 * • Results card (if sim data) — large primary value + secondary grid
 * • Static properties — element-specific fields
 * • Connections — connected links (for nodes) or from/to nodes (for links)
 * • Footer actions — Open in editor
 */

import {
  MagnifyingGlassPlusIcon,
  PencilSquareIcon,
  TrashIcon,
} from "@heroicons/react/16/solid";
import type React from "react";
import { useActiveProject } from "../../AppContext";
import type { LinkVariable, NodeVariable } from "../../canvas/types";
import {
  engineComponents,
  type GenericElementValue,
} from "../../engine/registry";
import type { Link, Node, ResultRanges } from "../../hooks";
import { ACCENT } from "../../hooks";
import { elementTypeBadge } from "../../types/elementTypes";
import type { Region } from "../../types/network";
import { Header } from "./ElementInspector/InspectorHeader";
import { LinkBody } from "./ElementInspector/LinkBody";
import { NodeBody } from "./ElementInspector/NodeBody";
import { LINK_TYPE_COLOR } from "./ElementInspector/ResultsCards";

const btnIcon: React.CSSProperties = {
  background: "var(--bg-card)",
  border: "1px solid var(--border)",
  color: "var(--text-secondary)",
  borderRadius: 6,
  padding: 6,
  cursor: "pointer",
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
};

// ── Public component: node variant ────────────────────────────────────────────

interface NodeInspectorProps {
  node: Node;
  onClose: () => void;
  onOpenInEditor: () => void;
  onZoomTo?: () => void;
  disableZoomTo?: boolean;
  onDelete?: () => void;
  onRename?: (newId: string) => void;
  onLocateRelated: (id: string) => void;
  onOpenPattern?: (id: string) => void;
  nodeVar?: NodeVariable;
  ranges?: ResultRanges;
  hasSimulation?: boolean;
  isTransitioning?: boolean;
  /** Current-period catalog values for engines with generic results —
   * consumed by the per-engine body slot; the wds body ignores it. */
  genericResults?: GenericElementValue[] | null;
}

export function NodeInspector({
  node,
  onClose,
  onOpenInEditor,
  onZoomTo,
  disableZoomTo,
  onDelete,
  onRename,
  onLocateRelated,
  onOpenPattern,
  nodeVar,
  ranges,
  hasSimulation,
  isTransitioning,
  genericResults,
}: NodeInspectorProps) {
  // The body is engine vocabulary (attributes + result cards) — selected
  // once from the registry; chrome (header, footer actions) stays shared.
  const { engine } = useActiveProject();
  const components = engineComponents(engine?.key);
  const EngineBody = components.NodeInspectorBody;
  const canOpenInEditor = components.editorFocusesElements;
  return (
    <div
      className="inspector-panel"
      style={{
        position: "absolute",
        right: 0,
        top: 0,
        bottom: 0,
        zIndex: 30,
        display: "flex",
        flexDirection: "column",
      }}
    >
      <Header
        id={node.id}
        subtitle={node.type}
        accentColor={ACCENT}
        badge={
          <div
            style={{
              width: 8,
              height: 8,
              borderRadius: "50%",
              background: elementTypeBadge(node.type).color,
              boxShadow: `0 0 6px ${elementTypeBadge(node.type).color}88`,
              flexShrink: 0,
            }}
          />
        }
        onClose={onClose}
        onRename={onRename}
      />

      {EngineBody ? (
        <EngineBody
          node={node}
          onLocateLink={onLocateRelated}
          results={genericResults}
        />
      ) : (
        <NodeBody
          node={node}
          accent={ACCENT}
          nodeVar={nodeVar}
          ranges={ranges}
          hasSimulation={hasSimulation}
          isTransitioning={isTransitioning}
          onOpenPattern={onOpenPattern}
          onLocateLink={onLocateRelated}
        />
      )}

      <div
        style={{
          flexShrink: 0,
          borderTop: "1px solid var(--border)",
          padding: 10,
          display: "flex",
          gap: 6,
        }}
      >
        {canOpenInEditor && (
          <button
            type="button"
            onClick={onOpenInEditor}
            data-tooltip="Open in editor"
            style={btnIcon}
          >
            <PencilSquareIcon style={{ width: 14, height: 14 }} />
          </button>
        )}
        {onZoomTo && (
          <button
            type="button"
            onClick={onZoomTo}
            disabled={disableZoomTo}
            data-tooltip="Zoom to feature"
            style={{
              ...btnIcon,
              opacity: disableZoomTo ? 0.45 : 1,
              cursor: disableZoomTo ? "not-allowed" : btnIcon.cursor,
            }}
          >
            <MagnifyingGlassPlusIcon style={{ width: 14, height: 14 }} />
          </button>
        )}
        {onDelete && (
          <button
            type="button"
            onClick={onDelete}
            data-tooltip="Delete element"
            style={{
              ...btnIcon,
              color: "var(--color-danger, #ef4444)",
              marginLeft: "auto",
            }}
          >
            <TrashIcon style={{ width: 14, height: 14 }} />
          </button>
        )}
      </div>
    </div>
  );
}

// ── Public component: link variant ────────────────────────────────────────────

interface LinkInspectorProps {
  link: Link;
  onClose: () => void;
  onOpenInEditor: () => void;
  onZoomTo?: () => void;
  disableZoomTo?: boolean;
  onDelete?: () => void;
  onRename?: (newId: string) => void;
  onLocateNode: (id: string) => void;
  linkVar?: LinkVariable;
  ranges?: ResultRanges;
  hasSimulation?: boolean;
  isTransitioning?: boolean;
  /** See NodeInspectorProps.genericResults. */
  genericResults?: GenericElementValue[] | null;
}

export function LinkInspector({
  link,
  onClose,
  onOpenInEditor,
  onZoomTo,
  disableZoomTo,
  onDelete,
  onRename,
  onLocateNode,
  linkVar,
  ranges,
  hasSimulation,
  isTransitioning,
  genericResults,
}: LinkInspectorProps) {
  // Same registry selection as NodeInspector — see the comment there.
  const { engine } = useActiveProject();
  const components = engineComponents(engine?.key);
  const EngineBody = components.LinkInspectorBody;
  const canOpenInEditor = components.editorFocusesElements;
  return (
    <div
      className="inspector-panel"
      style={{
        position: "absolute",
        right: 0,
        top: 0,
        bottom: 0,
        zIndex: 30,
        display: "flex",
        flexDirection: "column",
      }}
    >
      <Header
        id={link.id}
        subtitle={link.type}
        accentColor={ACCENT}
        badge={
          <div
            style={{
              width: 16,
              height: 3,
              borderRadius: 2,
              // The badge's kind colour — LINK_TYPE_COLOR only knows wds
              // kinds and the wds accent is engine identity, not a fallback.
              background:
                LINK_TYPE_COLOR[link.type] ?? elementTypeBadge(link.type).color,
              flexShrink: 0,
            }}
          />
        }
        onClose={onClose}
        onRename={onRename}
      />

      {EngineBody ? (
        <EngineBody
          link={link}
          onLocateNode={onLocateNode}
          results={genericResults}
        />
      ) : (
        <LinkBody
          link={link}
          accent={ACCENT}
          linkVar={linkVar}
          ranges={ranges}
          hasSimulation={hasSimulation}
          isTransitioning={isTransitioning}
          onLocateNode={onLocateNode}
        />
      )}

      <div
        style={{
          flexShrink: 0,
          borderTop: "1px solid var(--border)",
          padding: 10,
          display: "flex",
          gap: 6,
        }}
      >
        {canOpenInEditor && (
          <button
            type="button"
            onClick={onOpenInEditor}
            data-tooltip="Open in editor"
            style={btnIcon}
          >
            <PencilSquareIcon style={{ width: 14, height: 14 }} />
          </button>
        )}
        {onZoomTo && (
          <button
            type="button"
            onClick={onZoomTo}
            disabled={disableZoomTo}
            data-tooltip="Zoom to feature"
            style={{
              ...btnIcon,
              opacity: disableZoomTo ? 0.45 : 1,
              cursor: disableZoomTo ? "not-allowed" : btnIcon.cursor,
            }}
          >
            <MagnifyingGlassPlusIcon style={{ width: 14, height: 14 }} />
          </button>
        )}
        {onDelete && (
          <button
            type="button"
            onClick={onDelete}
            data-tooltip="Delete element"
            style={{
              ...btnIcon,
              color: "var(--color-danger, #ef4444)",
              marginLeft: "auto",
            }}
          >
            <TrashIcon style={{ width: 14, height: 14 }} />
          </button>
        )}
      </div>
    </div>
  );
}

// ── Public component: region variant ──────────────────────────────────────────

interface RegionInspectorProps {
  region: Region;
  onClose: () => void;
  onZoomTo?: () => void;
  /** Select the element this region discharges to. */
  onLocateOutlet: (id: string) => void;
  /** See NodeInspectorProps.genericResults. */
  genericResults?: GenericElementValue[] | null;
}

/**
 * Inspector for an areal element (a subcatchment). Same chrome as the node
 * and link variants — header with the kind badge, engine body, footer
 * actions — minus the affordances an area has no meaning for: there is no
 * "open in editor" (no engine edits areas here yet) and no rename/delete
 * (only read-only engines have areas today). Renders nothing when the
 * engine supplies no region body, which is also when nothing can select
 * one.
 */
export function RegionInspector({
  region,
  onClose,
  onZoomTo,
  onLocateOutlet,
  genericResults,
}: RegionInspectorProps) {
  const { engine } = useActiveProject();
  const EngineBody = engineComponents(engine?.key).RegionInspectorBody;
  if (!EngineBody) return null;
  return (
    <div
      className="inspector-panel"
      style={{
        position: "absolute",
        right: 0,
        top: 0,
        bottom: 0,
        zIndex: 30,
        display: "flex",
        flexDirection: "column",
      }}
    >
      <Header
        id={region.id}
        subtitle={region.type}
        accentColor={ACCENT}
        badge={
          <div
            style={{
              width: 12,
              height: 9,
              borderRadius: 2,
              border: `1.5px solid ${elementTypeBadge(region.type).color}`,
              background: `${elementTypeBadge(region.type).color}33`,
              flexShrink: 0,
            }}
          />
        }
        onClose={onClose}
      />

      <EngineBody
        region={region}
        onLocateOutlet={onLocateOutlet}
        results={genericResults}
      />

      {onZoomTo && (
        <div
          style={{
            flexShrink: 0,
            borderTop: "1px solid var(--border)",
            padding: 10,
            display: "flex",
            gap: 6,
          }}
        >
          <button
            type="button"
            onClick={onZoomTo}
            data-tooltip="Zoom to feature"
            style={btnIcon}
          >
            <MagnifyingGlassPlusIcon style={{ width: 14, height: 14 }} />
          </button>
        </div>
      )}
    </div>
  );
}
