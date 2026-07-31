/**
 * The report outline: the sections actually in the document, in order.
 *
 * Membership is add/remove rather than a checkbox, because that is what the
 * template format records — a block is listed or it is not. Rows carry the
 * document's own numbering so the outline reads as the report reads.
 *
 * Drag-reorder uses native HTML5 drag events rather than a library: the list
 * is short, vertical, and single-column, which is the one case native drag
 * handles well.
 */

import {
  Bars3Icon,
  Cog6ToothIcon,
  ExclamationTriangleIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import { useState } from "react";
import { ACCENT } from "../../../hooks";
import type {
  BlockAvailability,
  ReportBlockInfo,
  ReportOptionInfo,
} from "../../../hooks/reports";
import { BlockOptions, type OptionValues } from "./BlockOptions";

export interface SectionListProps {
  sections: string[];
  blockById: Map<string, ReportBlockInfo>;
  descriptorsById: Record<string, ReportOptionInfo[]>;
  optionsById: Record<string, unknown>;
  headingById: Record<string, string>;
  availabilityById: Map<string, BlockAvailability>;
  onReorder: (from: number, to: number) => void;
  onRemove: (id: string) => void;
  onOptionsChange: (id: string, next: OptionValues) => void;
  onHeadingChange: (id: string, heading: string) => void;
}

const rowButton = (active: boolean): React.CSSProperties => ({
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  width: 20,
  height: 20,
  padding: 0,
  borderRadius: 4,
  border: "none",
  background: "transparent",
  color: active ? ACCENT : "var(--text-tertiary)",
  cursor: "pointer",
  flexShrink: 0,
});

export function SectionList({
  sections,
  blockById,
  descriptorsById,
  optionsById,
  headingById,
  availabilityById,
  onReorder,
  onRemove,
  onOptionsChange,
  onHeadingChange,
}: SectionListProps) {
  const [openFor, setOpenFor] = useState<Set<string>>(new Set());
  const [dragging, setDragging] = useState<number | null>(null);
  const [dropTarget, setDropTarget] = useState<number | null>(null);

  if (sections.length === 0) {
    return (
      <p
        style={{
          fontSize: "var(--text-sm)",
          color: "var(--text-tertiary)",
          lineHeight: 1.5,
          margin: "4px 2px",
        }}
      >
        No sections yet. Add one to start building the report.
      </p>
    );
  }

  function toggleOpen(id: string) {
    setOpenFor((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
      {sections.map((id, index) => {
        const block = blockById.get(id);
        if (!block) return null;
        const descriptors = descriptorsById[id] ?? [];
        const open = openFor.has(id);
        const configured =
          Object.keys(
            (optionsById[id] as Record<string, unknown> | undefined) ?? {},
          ).length > 0;
        const availability = availabilityById.get(id);
        const problem = availability && availability.status !== "ok";
        const heading = headingById[id] ?? "";
        const isDropTarget = dropTarget === index && dragging !== index;

        return (
          <div
            key={id}
            onDragOver={(e) => {
              e.preventDefault();
              setDropTarget(index);
            }}
            onDrop={(e) => {
              e.preventDefault();
              if (dragging !== null) onReorder(dragging, index);
              setDragging(null);
              setDropTarget(null);
            }}
            style={{
              borderTop: isDropTarget
                ? `2px solid ${ACCENT}`
                : "2px solid transparent",
              opacity: dragging === index ? 0.4 : 1,
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "5px 6px",
                borderRadius: 6,
                background: "var(--bg-elevated)",
                border: "1px solid var(--border)",
              }}
            >
              <span
                draggable
                onDragStart={() => setDragging(index)}
                onDragEnd={() => {
                  setDragging(null);
                  setDropTarget(null);
                }}
                aria-label="Drag to reorder"
                title="Drag to reorder"
                style={{
                  display: "flex",
                  color: "var(--text-tertiary)",
                  cursor: "grab",
                  flexShrink: 0,
                }}
              >
                <Bars3Icon style={{ width: 12, height: 12 }} />
              </span>
              <span
                style={{
                  fontSize: "var(--text-sm)",
                  color: "var(--text-tertiary)",
                  fontVariantNumeric: "tabular-nums",
                  flexShrink: 0,
                }}
              >
                {index + 1}
              </span>
              <span
                title={block.summary}
                style={{
                  flex: 1,
                  fontSize: "var(--text-lg)",
                  color: "var(--text-primary)",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {heading.trim() || block.title}
              </span>
              {problem ? (
                <span
                  title={availability?.reason ?? "Will not render for this run"}
                  style={{ display: "flex", color: "var(--warn, #c98a1b)" }}
                >
                  <ExclamationTriangleIcon style={{ width: 12, height: 12 }} />
                </span>
              ) : null}
              <button
                type="button"
                aria-label={open ? "Hide settings" : "Settings"}
                title={open ? "Hide settings" : "Settings"}
                onClick={() => toggleOpen(id)}
                style={rowButton(open || configured || heading.trim() !== "")}
              >
                <Cog6ToothIcon style={{ width: 12, height: 12 }} />
              </button>
              <button
                type="button"
                aria-label={`Remove ${block.title}`}
                title="Remove from report"
                onClick={() => onRemove(id)}
                style={rowButton(false)}
              >
                <XMarkIcon style={{ width: 13, height: 13 }} />
              </button>
            </div>

            {problem ? (
              <p
                style={{
                  margin: "2px 0 0 34px",
                  fontSize: "var(--text-xs)",
                  color: "var(--text-tertiary)",
                  lineHeight: 1.4,
                }}
              >
                {availability?.reason ??
                  "This section will not render for this run."}
              </p>
            ) : null}

            {open ? (
              <div
                style={{
                  padding: "8px 8px 2px 30px",
                  borderLeft: "2px solid var(--border)",
                  marginLeft: 10,
                  marginBottom: 4,
                }}
              >
                <label style={{ display: "block", marginBottom: 10 }}>
                  <span
                    style={{
                      display: "block",
                      fontSize: "var(--text-sm)",
                      color: "var(--text-secondary)",
                      marginBottom: 3,
                    }}
                  >
                    Heading
                  </span>
                  <input
                    type="text"
                    value={heading}
                    placeholder={block.title}
                    onChange={(e) => onHeadingChange(id, e.target.value)}
                    style={{
                      width: "100%",
                      padding: "4px 6px",
                      borderRadius: 4,
                      border: "1px solid var(--border)",
                      background: "var(--bg-base)",
                      color: "var(--text-primary)",
                      fontSize: "var(--text-sm)",
                      fontFamily: "var(--font-ui)",
                    }}
                  />
                  <span
                    style={{
                      display: "block",
                      fontSize: "var(--text-xs)",
                      color: "var(--text-tertiary)",
                      marginTop: 3,
                    }}
                  >
                    Replaces this section's heading in the document. Leave empty
                    to keep the default.
                  </span>
                </label>
                <BlockOptions
                  descriptors={descriptors}
                  values={optionsById[id] as OptionValues}
                  onChange={(next) => onOptionsChange(id, next)}
                />
              </div>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
