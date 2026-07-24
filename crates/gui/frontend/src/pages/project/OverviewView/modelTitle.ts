/**
 * Helpers between the INP `[TITLE]` line array and the Overview header's
 * free-text editor. EPANET treats three lines as convention, not a rule, so
 * any line count round-trips; the DISPLAY clamps to three lines with a
 * "View more" toggle.
 */

/** Number of lines shown collapsed before the "View more" toggle appears. */
export const TITLE_DISPLAY_LINES = 3;

/** Join title lines for the textarea. */
export function titleLinesToText(lines: string[]): string {
  return lines.join("\n");
}

/** Split textarea text into title lines: trailing whitespace per line and
 * trailing empty lines drop, interior empties survive. */
export function textToTitleLines(text: string): string[] {
  const lines = text.split("\n").map((l) => l.trimEnd());
  while (lines.length > 0 && lines[lines.length - 1] === "") lines.pop();
  return lines;
}
