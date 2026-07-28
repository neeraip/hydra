/**
 * Truncate a long identifier in the *middle* rather than at the end.
 *
 * Real network IDs share long prefixes (`WMTR-G1209`, `WMTR-G1206`, …), so
 * end-truncation elides the only part that tells two rows apart: a whole
 * table of them renders as `WMTR-…` repeated. Keeping a fixed tail visible
 * and eliding the head shows the discriminating characters instead.
 *
 * Implemented with two spans rather than a JS character budget so it stays
 * correct at any column width, with any font, with no measurement pass: the
 * head flexes and ellipsises, the tail never shrinks. When the whole string
 * fits, nothing is elided and the output is indistinguishable from plain text.
 */

/** Characters kept visible at the end. Long enough for a numeric suffix. */
const DEFAULT_TAIL = 6;

export function MiddleTruncate({
  text,
  tailChars = DEFAULT_TAIL,
  title,
}: {
  text: string;
  /** Characters pinned at the end; the rest flexes and elides. */
  tailChars?: number;
  /** Native tooltip text; defaults to the untruncated value. */
  title?: string;
}) {
  // Short enough to always fit: skip the split so selection and copy behave
  // exactly like a plain text node.
  if (text.length <= tailChars) {
    return <span title={title ?? text}>{text}</span>;
  }
  const head = text.slice(0, text.length - tailChars);
  const tail = text.slice(text.length - tailChars);
  return (
    <span
      title={title ?? text}
      style={{ display: "flex", minWidth: 0, alignItems: "baseline" }}
    >
      <span
        style={{
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
          minWidth: 0,
        }}
      >
        {head}
      </span>
      <span style={{ whiteSpace: "nowrap", flexShrink: 0 }}>{tail}</span>
    </span>
  );
}
