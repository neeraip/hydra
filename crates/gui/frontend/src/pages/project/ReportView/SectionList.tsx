/**
 * The report outline: the sections actually in the document, in order.
 *
 * Membership is add/remove rather than a checkbox, because that is what the
 * template format records — a block is listed or it is not. Rows carry the
 * document's own numbering so the outline reads as the report reads.
 *
 * Drag-reorder is built on POINTER events, not HTML5 drag-and-drop. Tauri's
 * window claims OS drag events for file-drop (`dragDropEnabled`, on by
 * default), so `dragstart` never reaches the webview reliably — and turning
 * that off to suit one list would cost the whole app its ability to accept a
 * dropped model file. Pointer events are also platform-uniform and work with
 * touch, at the cost of computing the drop slot ourselves.
 */

import {
  Bars3Icon,
  Cog6ToothIcon,
  ExclamationTriangleIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ACCENT } from "../../../hooks";
import {
  type BlockAvailability,
  customisedSummary,
  insertionFromPointer,
  insertionToIndex,
  type ReportBlockInfo,
  type ReportOptionInfo,
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
  // The row being dragged, the gap it would drop into, and the geometry the
  // floating copy needs to follow the pointer.
  const [drag, setDrag] = useState<{
    from: number;
    insertion: number;
    pointerY: number;
    /** Where inside the row it was grabbed, so that point stays under the
     * cursor instead of the copy snapping its top edge to the pointer. */
    grabOffsetY: number;
    left: number;
    width: number;
  } | null>(null);
  // Row bounds, measured once at grab: the list does not reflow during a
  // drag (the indicator moves, the rows do not), so re-measuring per move
  // would be wasted work and would fight the pointer.
  const rowRects = useRef<{ top: number; height: number }[]>([]);
  const rowRefs = useRef<(HTMLDivElement | null)[]>([]);

  // The cursor otherwise reverts to whatever sits under the pointer once it
  // leaves the handle, which reads as the drag having been dropped.
  useEffect(() => {
    if (!drag) return;
    const previous = document.body.style.cursor;
    document.body.style.cursor = "grabbing";
    return () => {
      document.body.style.cursor = previous;
    };
  }, [drag]);

  function beginDrag(index: number, e: React.PointerEvent) {
    e.preventDefault();
    // Slice to the live rows: refs for removed sections stay in the array,
    // and measuring them would add phantom slots below the list.
    rowRects.current = rowRefs.current.slice(0, sections.length).map((el) => {
      const r = el?.getBoundingClientRect();
      return { top: r?.top ?? 0, height: r?.height ?? 0 };
    });
    const rect = rowRefs.current[index]?.getBoundingClientRect();
    e.currentTarget.setPointerCapture(e.pointerId);
    setDrag({
      from: index,
      insertion: index,
      pointerY: e.clientY,
      grabOffsetY: e.clientY - (rect?.top ?? e.clientY),
      left: rect?.left ?? 0,
      width: rect?.width ?? 0,
    });
  }

  function moveDrag(e: React.PointerEvent) {
    if (!drag) return;
    setDrag({
      ...drag,
      pointerY: e.clientY,
      insertion: insertionFromPointer(rowRects.current, e.clientY),
    });
  }

  function endDrag() {
    if (drag) {
      const to = insertionToIndex(drag.from, drag.insertion);
      if (to !== drag.from) onReorder(drag.from, to);
    }
    setDrag(null);
  }

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
        const availability = availabilityById.get(id);
        const problem = availability && availability.status !== "ok";
        const heading = headingById[id] ?? "";
        // What has been changed from the engine's defaults, by name — the
        // marker is useless if it cannot say why it is lit.
        const customised = customisedSummary(
          descriptors,
          optionsById[id] as Record<string, unknown> | undefined,
          heading,
        );
        // The indicator sits in the gap the row would drop into, and is
        // hidden when that gap is where the row already is.
        const showGap =
          drag !== null &&
          drag.insertion === index &&
          insertionToIndex(drag.from, drag.insertion) !== drag.from;

        return (
          <div
            key={id}
            ref={(el) => {
              rowRefs.current[index] = el;
            }}
            style={{
              borderTop: showGap
                ? `2px solid ${ACCENT}`
                : "2px solid transparent",
              opacity: drag?.from === index ? 0.45 : 1,
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
                onPointerDown={(e) => beginDrag(index, e)}
                onPointerMove={moveDrag}
                onPointerUp={endDrag}
                onPointerCancel={endDrag}
                aria-label="Drag to reorder"
                data-tooltip="Drag to reorder"
                style={{
                  display: "flex",
                  color: "var(--text-tertiary)",
                  cursor: drag ? "grabbing" : "grab",
                  flexShrink: 0,
                  // Stops the gesture being stolen by scrolling on touch and
                  // by text selection with a mouse.
                  touchAction: "none",
                  userSelect: "none",
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
                data-tooltip={block.summary}
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
                  data-tooltip={
                    availability?.reason ?? "Will not render for this run"
                  }
                  style={{ display: "flex", color: "var(--warn, #c98a1b)" }}
                >
                  <ExclamationTriangleIcon style={{ width: 12, height: 12 }} />
                </span>
              ) : null}
              <button
                type="button"
                aria-label={open ? "Hide settings" : "Settings"}
                data-tooltip={
                  customised.length > 0
                    ? `Changed from defaults: ${customised.join(", ")}`
                    : open
                      ? "Hide settings"
                      : "Settings"
                }
                onClick={() => toggleOpen(id)}
                // Tinted for CUSTOMISED only, never for open: the expanded
                // panel already shows that it is open, so spending the same
                // signal on both left the marker meaning nothing in
                // particular.
                style={rowButton(customised.length > 0)}
              >
                <Cog6ToothIcon style={{ width: 12, height: 12 }} />
              </button>
              <button
                type="button"
                aria-label={`Remove ${block.title}`}
                data-tooltip="Remove from report"
                onClick={() => onRemove(id)}
                style={rowButton(false)}
              >
                <XMarkIcon style={{ width: 13, height: 13 }} />
              </button>
            </div>

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
                {customised.length > 0 ? (
                  <button
                    type="button"
                    onClick={() => {
                      onOptionsChange(id, undefined);
                      onHeadingChange(id, "");
                    }}
                    style={{
                      padding: "2px 0",
                      marginBottom: 8,
                      border: "none",
                      background: "transparent",
                      color: ACCENT,
                      cursor: "pointer",
                      fontSize: "var(--text-sm)",
                      fontFamily: "var(--font-ui)",
                    }}
                  >
                    Reset to defaults
                  </button>
                ) : null}
              </div>
            ) : null}
          </div>
        );
      })}
      {/* Slot after the last row: it belongs to no row, so it needs its own
          indicator or dropping at the end reads as nothing happening. */}
      <div
        style={{
          borderTop:
            drag !== null &&
            drag.insertion === sections.length &&
            drag.from !== sections.length - 1
              ? `2px solid ${ACCENT}`
              : "2px solid transparent",
        }}
      />
      <DragGhost
        drag={drag}
        sections={sections}
        blockById={blockById}
        headingById={headingById}
      />
    </div>
  );
}

/**
 * The row following the cursor while dragging.
 *
 * Pointer-based reordering has no native drag image, so the copy is drawn
 * here. It carries the row's measured width and left edge so it tracks
 * vertically without drifting sideways, and renders through a portal because
 * the rail scrolls — a copy inside it would be clipped at the rail's edge the
 * moment the pointer left the list.
 *
 * Deliberately not a pixel-perfect copy: the handle, number and heading are
 * what identify the row, and reproducing the action buttons would mean
 * rendering controls that cannot be clicked.
 */
function DragGhost({
  drag,
  sections,
  blockById,
  headingById,
}: {
  drag: {
    from: number;
    pointerY: number;
    grabOffsetY: number;
    left: number;
    width: number;
  } | null;
  sections: string[];
  blockById: Map<string, ReportBlockInfo>;
  headingById: Record<string, string>;
}) {
  if (!drag) return null;
  const id = sections[drag.from];
  const block = id ? blockById.get(id) : undefined;
  if (!block) return null;
  const heading = headingById[id] ?? "";

  return createPortal(
    <div
      style={{
        position: "fixed",
        left: drag.left,
        top: drag.pointerY - drag.grabOffsetY,
        width: drag.width,
        pointerEvents: "none",
        zIndex: 1000,
        display: "flex",
        alignItems: "center",
        gap: 6,
        padding: "5px 6px",
        borderRadius: 6,
        background: "var(--bg-elevated)",
        border: `1px solid ${ACCENT}`,
        boxShadow: "0 6px 16px rgba(0, 0, 0, 0.28)",
        fontFamily: "var(--font-ui)",
        opacity: 0.95,
      }}
    >
      <Bars3Icon
        style={{
          width: 12,
          height: 12,
          color: "var(--text-tertiary)",
          flexShrink: 0,
        }}
      />
      <span
        style={{
          fontSize: "var(--text-sm)",
          color: "var(--text-tertiary)",
          fontVariantNumeric: "tabular-nums",
          flexShrink: 0,
        }}
      >
        {drag.from + 1}
      </span>
      <span
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
    </div>,
    document.body,
  );
}
