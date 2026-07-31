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
import { useRef, useState } from "react";
import { ACCENT } from "../../../hooks";
import {
  type BlockAvailability,
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
  // Which row is being dragged, and the gap it would drop into.
  const [drag, setDrag] = useState<{ from: number; insertion: number } | null>(
    null,
  );
  // Row bounds, measured once at grab: the list does not reflow during a
  // drag (the indicator moves, the rows do not), so re-measuring per move
  // would be wasted work and would fight the pointer.
  const rowRects = useRef<{ top: number; height: number }[]>([]);
  const rowRefs = useRef<(HTMLDivElement | null)[]>([]);

  function beginDrag(index: number, e: React.PointerEvent) {
    e.preventDefault();
    // Slice to the live rows: refs for removed sections stay in the array,
    // and measuring them would add phantom slots below the list.
    rowRects.current = rowRefs.current.slice(0, sections.length).map((el) => {
      const r = el?.getBoundingClientRect();
      return { top: r?.top ?? 0, height: r?.height ?? 0 };
    });
    e.currentTarget.setPointerCapture(e.pointerId);
    setDrag({ from: index, insertion: index });
  }

  function moveDrag(e: React.PointerEvent) {
    if (!drag) return;
    setDrag({
      from: drag.from,
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
        const configured =
          Object.keys(
            (optionsById[id] as Record<string, unknown> | undefined) ?? {},
          ).length > 0;
        const availability = availabilityById.get(id);
        const problem = availability && availability.status !== "ok";
        const heading = headingById[id] ?? "";
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
                title="Drag to reorder"
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
    </div>
  );
}
