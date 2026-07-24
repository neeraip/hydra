/* Shared virtualized sidebar list for the Curves/Patterns editors.
   Networks can carry thousands of patterns (per-junction demand patterns)
   or curves, and the sidebars used to mount one button per entry — the same
   unbounded-DOM failure class that froze the Issues panel. Rows are
   virtualized with the IssuesPanel pattern (useVirtualizer + measureElement);
   the create-new affordance renders below the list and stays mounted. */

import { useVirtualizer } from "@tanstack/react-virtual";
import type React from "react";
import { useRef } from "react";

/** Estimated sidebar row height: 10px vertical padding ×2 + two text lines. */
const SIDEBAR_ROW_ESTIMATE = 56;

export function EditorSidebarList<T>({
  items,
  getKey,
  renderItem,
  footer,
}: {
  items: T[];
  getKey: (item: T) => string;
  /** Renders one entry (typically a full-width button with its own borders). */
  renderItem: (item: T) => React.ReactNode;
  /** Rendered below the virtualized list (e.g. the "+ New …" affordance). */
  footer: React.ReactNode;
}) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => SIDEBAR_ROW_ESTIMATE,
    overscan: 10,
    getItemKey: (index) => getKey(items[index]),
  });

  return (
    <div
      ref={scrollRef}
      style={{
        width: 220,
        borderRight: "1px solid var(--border)",
        overflow: "auto",
        flexShrink: 0,
      }}
    >
      <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
        {virtualizer.getVirtualItems().map((vi) => (
          <div
            key={vi.key}
            ref={virtualizer.measureElement}
            data-index={vi.index}
            style={{
              position: "absolute",
              top: 0,
              left: 0,
              width: "100%",
              transform: `translateY(${vi.start}px)`,
            }}
          >
            {renderItem(items[vi.index])}
          </div>
        ))}
      </div>
      {footer}
    </div>
  );
}
