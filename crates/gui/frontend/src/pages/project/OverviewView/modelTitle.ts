/**
 * Split/join helpers between the INP `[TITLE]` line array and the Overview
 * card's title + description fields. EPANET convention: up to three lines —
 * line 1 is the title, lines 2-3 the description.
 */

export interface ModelTitleParts {
  title: string;
  description: string;
}

/** First line → title; remaining lines (≤2) joined with a newline. */
export function titleLinesToParts(lines: string[]): ModelTitleParts {
  return {
    title: lines[0] ?? "",
    description: lines.slice(1).join("\n"),
  };
}

/**
 * Rebuild the `[TITLE]` line array. The description contributes at most two
 * lines (extra newlines collapse into the second description line); trailing
 * empty lines are dropped so an empty card produces an empty title.
 */
export function partsToTitleLines(parts: ModelTitleParts): string[] {
  const descLines = parts.description.split("\n").map((l) => l.trimEnd());
  const capped =
    descLines.length <= 2
      ? descLines
      : [descLines[0], descLines.slice(1).join(" ").trim()];
  const lines = [parts.title.trimEnd(), ...capped];
  while (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}
