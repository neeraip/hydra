/**
 * The "add a section" palette.
 *
 * Shows each block's summary as body text rather than a hover tooltip: the
 * whole decision here is "what does this section contain", and hiding that
 * behind a hover is what made the old checkbox list guesswork. Availability
 * is shown too, so a section that cannot render for this run is a visible
 * choice rather than a placeholder discovered later in the preview.
 *
 * Only blocks not already in the report are listed — adding is the sole
 * action, so the list shrinks as the report is built.
 */

import { MagnifyingGlassIcon } from "@heroicons/react/16/solid";
import { useEffect, useMemo, useRef, useState } from "react";
import { ACCENT } from "../../../hooks";
import {
  addableBlocks,
  type BlockAvailability,
  type ReportBlockInfo,
} from "../../../hooks/reports";

export function AddSectionPalette({
  catalog,
  sections,
  availabilityById,
  onAdd,
  onClose,
}: {
  catalog: ReportBlockInfo[];
  sections: string[];
  availabilityById: Map<string, BlockAvailability>;
  onAdd: (id: string) => void;
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => inputRef.current?.focus(), []);

  const matches = useMemo(
    () => addableBlocks(catalog, sections, query),
    [catalog, sections, query],
  );

  return (
    <search
      onKeyDown={(e) => {
        if (e.key === "Escape") onClose();
      }}
      style={{
        border: "1px solid var(--border-hover)",
        borderRadius: 8,
        background: "var(--bg-app)",
        padding: 8,
        display: "flex",
        flexDirection: "column",
        gap: 6,
        maxHeight: 320,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <MagnifyingGlassIcon
          style={{ width: 12, height: 12, color: "var(--text-tertiary)" }}
        />
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search sections"
          spellCheck={false}
          style={{
            flex: 1,
            padding: "4px 2px",
            border: "none",
            background: "transparent",
            color: "var(--text-primary)",
            fontSize: "var(--text-sm)",
            fontFamily: "var(--font-ui)",
            outline: "none",
          }}
        />
      </div>

      <div
        style={{
          overflowY: "auto",
          display: "flex",
          flexDirection: "column",
          gap: 2,
        }}
      >
        {matches.length === 0 ? (
          <p
            style={{
              fontSize: "var(--text-sm)",
              color: "var(--text-tertiary)",
              margin: "6px 2px",
              lineHeight: 1.45,
            }}
          >
            {sections.length === catalog.length
              ? "Every section is already in the report."
              : "No section matches that search."}
          </p>
        ) : (
          matches.map((block) => {
            const availability = availabilityById.get(block.id);
            const problem = availability && availability.status !== "ok";
            return (
              <button
                key={block.id}
                type="button"
                onClick={() => onAdd(block.id)}
                style={{
                  textAlign: "left",
                  padding: "6px 7px",
                  borderRadius: 6,
                  border: "1px solid transparent",
                  background: "transparent",
                  cursor: "pointer",
                  fontFamily: "var(--font-ui)",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.background = "var(--bg-elevated)";
                  e.currentTarget.style.borderColor = "var(--border)";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.background = "transparent";
                  e.currentTarget.style.borderColor = "transparent";
                }}
              >
                <span
                  style={{
                    display: "block",
                    fontSize: "var(--text-lg)",
                    color: "var(--text-primary)",
                    marginBottom: 2,
                  }}
                >
                  {block.title}
                </span>
                <span
                  style={{
                    display: "block",
                    fontSize: "var(--text-xs)",
                    color: "var(--text-tertiary)",
                    lineHeight: 1.4,
                  }}
                >
                  {block.summary}
                </span>
                {problem ? (
                  <span
                    style={{
                      display: "block",
                      fontSize: "var(--text-xs)",
                      color: "var(--warn, #c98a1b)",
                      marginTop: 3,
                      lineHeight: 1.4,
                    }}
                  >
                    {availability?.reason ?? "Will not render for this run"}
                  </span>
                ) : null}
              </button>
            );
          })
        )}
      </div>

      <button
        type="button"
        onClick={onClose}
        style={{
          alignSelf: "flex-start",
          padding: "2px 4px",
          border: "none",
          background: "transparent",
          color: ACCENT,
          cursor: "pointer",
          fontSize: "var(--text-sm)",
          fontFamily: "var(--font-ui)",
        }}
      >
        Done
      </button>
    </search>
  );
}
