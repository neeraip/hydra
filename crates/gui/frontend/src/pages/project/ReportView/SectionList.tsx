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
  ChevronRightIcon,
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
  rowShift,
} from "../../../hooks/reports";
import { BlockOptions, type OptionValues } from "./BlockOptions";

export interface SectionListProps {
  sections: string[];
  blockById: Map<string, ReportBlockInfo>;
  descriptorsById: Record<string, ReportOptionInfo[]>;
  optionsById: Record<string, unknown>;
  headingById: Record<string, string>;
  availabilityById: Map<string, BlockAvailability>;
  /** Which sections have their settings panel open. Owned by the caller so
   * the Sections menu can expand or collapse the whole list. */
  openSections: ReadonlySet<string>;
  onToggleOpen: (id: string) => void;
  onReorder: (from: number, to: number) => void;
  onRemove: (id: string) => void;
  onOptionsChange: (id: string, next: OptionValues) => void;
  onHeadingChange: (id: string, heading: string) => void;
}

/** Vertical gap between rows. Shared by the container's `gap` and the
 * displacement maths — the space a lifted row frees is its own height PLUS
 * one gap, so the two must not be able to drift apart. */
const ROW_GAP = 2;
const SHIFT_MS = 160;
const DISCLOSE_MS = 120;

/** The section's real name, shown above a heading that has replaced it. */
const faintLine: React.CSSProperties = {
  fontSize: "var(--text-xs)",
  color: "var(--text-tertiary)",
  lineHeight: 1.1,
  overflow: "hidden",
  textOverflow: "ellipsis",
  whiteSpace: "nowrap",
};

const rowButton: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  width: 20,
  height: 20,
  padding: 0,
  borderRadius: 4,
  border: "none",
  background: "transparent",
  color: "var(--text-tertiary)",
  cursor: "pointer",
  flexShrink: 0,
};

export function SectionList({
  sections,
  blockById,
  descriptorsById,
  optionsById,
  headingById,
  availabilityById,
  openSections,
  onToggleOpen,
  onReorder,
  onRemove,
  onOptionsChange,
  onHeadingChange,
}: SectionListProps) {
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
    /** How far the other rows move: exactly the space this row frees, which
     * is its own height regardless of how tall its neighbours are (an open
     * settings panel makes rows wildly uneven). */
    height: number;
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
      height: rect?.height ?? 0,
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

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: ROW_GAP }}>
      {sections.map((id, index) => {
        const block = blockById.get(id);
        if (!block) return null;
        const descriptors = descriptorsById[id] ?? [];
        const open = openSections.has(id);
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
        // Rows between the dragged row's origin and its destination step
        // aside by exactly the space it frees, opening the gap it would drop
        // into. The dragged row keeps its slot but renders invisible, so that
        // freed space is real rather than simulated.
        const dragged = drag?.from === index;
        const offset = drag
          ? rowShift(
              index,
              drag.from,
              insertionToIndex(drag.from, drag.insertion),
              drag.height + ROW_GAP,
            )
          : 0;

        return (
          <div
            key={id}
            ref={(el) => {
              rowRefs.current[index] = el;
            }}
            style={{
              transform: offset === 0 ? undefined : `translateY(${offset}px)`,
              // Only while dragging: on drop the list re-renders in its new
              // order, and animating from the old offsets to zero would show
              // every row sliding back through a position it never held.
              transition: drag ? `transform ${SHIFT_MS}ms ease` : undefined,
              opacity: dragged ? 0 : 1,
              // The lifted row must not swallow pointer events aimed at what
              // is now visually in its place.
              pointerEvents: dragged ? "none" : undefined,
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
              {/* Everything between the handle and remove is one button, so
                  the row's body opens the settings. A real button rather than
                  a click handler on the row: it carries the expanded state and
                  keyboard operation without hand-rolled ARIA, and it cannot
                  swallow the two controls that must do something else. */}
              <button
                type="button"
                aria-expanded={open}
                onClick={() => onToggleOpen(id)}
                style={{
                  flex: 1,
                  minWidth: 0,
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  padding: 0,
                  border: "none",
                  background: "transparent",
                  cursor: "pointer",
                  textAlign: "left",
                  fontFamily: "var(--font-ui)",
                }}
              >
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
                {/* An overridden heading replaces the section's name in the
                    document, so it leads here too — with the name it replaced
                    kept above it, since the outline is otherwise the only
                    place that still says which block a renamed section is. */}
                <span
                  data-tooltip={block.summary}
                  style={{
                    flex: 1,
                    minWidth: 0,
                    display: "flex",
                    flexDirection: "column",
                    overflow: "hidden",
                  }}
                >
                  {heading.trim() ? (
                    <span style={{ ...faintLine, marginBottom: 1 }}>
                      {block.title}
                    </span>
                  ) : null}
                  <span
                    style={{
                      fontSize: "var(--text-lg)",
                      color: "var(--text-primary)",
                      lineHeight: 1.2,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {heading.trim() || block.title}
                  </span>
                </span>
                {problem ? (
                  <span
                    data-tooltip={
                      availability?.reason ?? "Will not render for this run"
                    }
                    style={{ display: "flex", color: "var(--warn, #c98a1b)" }}
                  >
                    <ExclamationTriangleIcon
                      style={{ width: 12, height: 12 }}
                    />
                  </span>
                ) : null}
                {/* The customised marker outlived the gear it used to tint.
                    It is a plain dot, not a control: opening the settings is
                    now the row's job, and a second clickable thing here would
                    just be a smaller target for the same action. */}
                {customised.length > 0 ? (
                  <span
                    data-tooltip={`Changed from defaults: ${customised.join(", ")}`}
                    style={{
                      width: 6,
                      height: 6,
                      borderRadius: "50%",
                      background: ACCENT,
                      flexShrink: 0,
                    }}
                  />
                ) : null}
                {/* Sits where the gear did, next to the row's other controls.
                    Rotated rather than swapped for a down-chevron: the turn is
                    what conveys that this row did the opening, where a swap
                    would just be a different icon appearing. */}
                <ChevronRightIcon
                  style={{
                    width: 11,
                    height: 11,
                    flexShrink: 0,
                    color: "var(--text-tertiary)",
                    transform: open ? "rotate(90deg)" : undefined,
                    transition: `transform ${DISCLOSE_MS}ms ease`,
                  }}
                />
              </button>
              <button
                type="button"
                aria-label={`Remove ${block.title}`}
                data-tooltip="Remove from report"
                onClick={() => onRemove(id)}
                style={rowButton}
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
          minWidth: 0,
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
        }}
      >
        {heading.trim() ? (
          <span style={{ ...faintLine, marginBottom: 1 }}>{block.title}</span>
        ) : null}
        <span
          style={{
            fontSize: "var(--text-lg)",
            color: "var(--text-primary)",
            lineHeight: 1.2,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {heading.trim() || block.title}
        </span>
      </span>
    </div>,
    document.body,
  );
}
