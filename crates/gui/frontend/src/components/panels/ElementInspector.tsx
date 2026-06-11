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
import type { LinkVariable, NodeVariable } from "../../canvas/types";
import type { Link, Node, ResultRanges } from "../../hooks";
import { ACCENT } from "../../hooks";
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
  onDelete?: () => void;
  onLocateRelated: (id: string) => void;
  onOpenPattern?: (id: string) => void;
  nodeVar?: NodeVariable;
  ranges?: ResultRanges;
  hasSimulation?: boolean;
  isTransitioning?: boolean;
}

export function NodeInspector({
  node,
  onClose,
  onOpenInEditor,
  onZoomTo,
  onDelete,
  onLocateRelated,
  onOpenPattern,
  nodeVar,
  ranges,
  hasSimulation,
  isTransitioning,
}: NodeInspectorProps) {
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
              background: ACCENT,
              boxShadow: `0 0 6px ${ACCENT}88`,
              flexShrink: 0,
            }}
          />
        }
        onClose={onClose}
      />

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
          onClick={onOpenInEditor}
          data-tooltip="Open in editor"
          style={btnIcon}
        >
          <PencilSquareIcon style={{ width: 14, height: 14 }} />
        </button>
        {onZoomTo && (
          <button
            onClick={onZoomTo}
            data-tooltip="Zoom to feature"
            style={btnIcon}
          >
            <MagnifyingGlassPlusIcon style={{ width: 14, height: 14 }} />
          </button>
        )}
        {onDelete && (
          <button
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
  onDelete?: () => void;
  onLocateNode: (id: string) => void;
  linkVar?: LinkVariable;
  ranges?: ResultRanges;
  hasSimulation?: boolean;
  isTransitioning?: boolean;
}

export function LinkInspector({
  link,
  onClose,
  onOpenInEditor,
  onZoomTo,
  onDelete,
  onLocateNode,
  linkVar,
  ranges,
  hasSimulation,
  isTransitioning,
}: LinkInspectorProps) {
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
              background: LINK_TYPE_COLOR[link.type] ?? ACCENT,
              flexShrink: 0,
            }}
          />
        }
        onClose={onClose}
      />

      <LinkBody
        link={link}
        accent={ACCENT}
        linkVar={linkVar}
        ranges={ranges}
        hasSimulation={hasSimulation}
        isTransitioning={isTransitioning}
        onLocateNode={onLocateNode}
      />

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
          onClick={onOpenInEditor}
          data-tooltip="Open in editor"
          style={btnIcon}
        >
          <PencilSquareIcon style={{ width: 14, height: 14 }} />
        </button>
        {onZoomTo && (
          <button
            onClick={onZoomTo}
            data-tooltip="Zoom to feature"
            style={btnIcon}
          >
            <MagnifyingGlassPlusIcon style={{ width: 14, height: 14 }} />
          </button>
        )}
        {onDelete && (
          <button
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
