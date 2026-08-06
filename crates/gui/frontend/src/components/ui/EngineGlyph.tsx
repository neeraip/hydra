/**
 * The mark that says which engine a thing belongs to.
 *
 * Five surfaces built this by hand from `engine.pill` and an inline
 * colour: the home page, the projects table, the status bar, the run
 * modal and the simulation settings modal. They had already drifted, with
 * three using an alpha-hex tint on the accent and two not.
 *
 * That is the same failure the element badge rule describes, for the same
 * reason: a mark assembled at each call site stops matching the others,
 * and a reader learns one vocabulary per screen instead of one.
 */

import type React from "react";
import type { EngineInfo } from "../../hooks";

/** How prominent the mark is. `sm` suits a table row, `md` a header. */
export type GlyphSize = "sm" | "md";

const SIZES: Record<GlyphSize, React.CSSProperties> = {
  sm: { fontSize: "var(--text-2xs)", padding: "1px 5px", borderRadius: 3 },
  md: { fontSize: "var(--text-xs)", padding: "2px 6px", borderRadius: 4 },
};

/**
 * @param engine the engine, or `null` where a project names one this build
 *               does not have. That case is shown rather than hidden: a
 *               project whose engine is missing is a thing the reader
 *               needs to know about.
 */
export function EngineGlyph({
  engine,
  size = "md",
}: {
  engine: EngineInfo | null;
  size?: GlyphSize;
}) {
  const accent = engine?.accent ?? "var(--text-tertiary)";
  return (
    <span
      // `role="img"` so the label is announced. Without a role this is a
      // generic span, which never reaches the accessibility tree, and the
      // two-letter mark would be read out as two letters.
      role="img"
      title={engine ? engine.label : "Unsupported engine"}
      aria-label={engine ? engine.label : "Unsupported engine"}
      style={{
        ...SIZES[size],
        fontWeight: 700,
        letterSpacing: "0.06em",
        color: accent,
        // Tinted from the accent rather than given a colour of its own, so
        // adding an engine needs one accent and nothing else.
        background: engine ? `${engine.accent}22` : "transparent",
        border: `1px solid ${engine ? `${engine.accent}44` : "var(--border)"}`,
        flexShrink: 0,
        whiteSpace: "nowrap",
      }}
    >
      {engine?.pill ?? "??"}
    </span>
  );
}
