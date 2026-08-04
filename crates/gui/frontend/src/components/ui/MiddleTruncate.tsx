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
 * head flexes and ellipsises down to a floor that keeps the ellipsis visible,
 * the tail never shrinks. When the whole string fits, nothing is elided and
 * the output is indistinguishable from plain text.
 */

/** Characters kept visible at the end. Long enough for a numeric suffix. */
const DEFAULT_TAIL = 6;

/**
 * Floor on the eliding head, wide enough to always paint the ellipsis.
 *
 * With `min-width: 0` the head is a flex item that shrinks to nothing at a
 * narrow column, and `text-overflow: ellipsis` needs room for the glyph — so
 * it painted nothing at all. `WMTR-G1209` then rendered as `-G1209`,
 * indistinguishable from an id that is genuinely six characters long. An
 * ellipsis that disappears is worse than one that crowds: it turns a
 * truncation into a lie.
 *
 * In `ch` so it tracks the font: the ids render in the monospace face, where
 * one `ch` is one character.
 */
const MIN_HEAD_CHARS = 2;
const MIN_HEAD = `${MIN_HEAD_CHARS}ch`;

/**
 * Split an id into an eliding head and a pinned tail, or `null` to render
 * it as plain text.
 *
 * Plain text when the id is short enough to always fit — selection and copy
 * then behave exactly like a text node — and, critically, when the head
 * would be narrower than {@link MIN_HEAD_CHARS}. The head is floored at
 * that width so the ellipsis always has room, which means a *shorter* head
 * is padded out to it, opening a gap that reads as part of the id:
 * `Street1` split into a one-character head rendered as "S treet1". Such a
 * head could never elide anyway — it always fits — so splitting it buys
 * nothing and costs a lie.
 */
export function splitForTruncation(
  text: string,
  tailChars: number,
): { head: string; tail: string } | null {
  if (text.length <= tailChars + MIN_HEAD_CHARS) return null;
  return {
    head: text.slice(0, text.length - tailChars),
    tail: text.slice(text.length - tailChars),
  };
}

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
  const split = splitForTruncation(text, tailChars);
  if (!split) {
    return <span title={title ?? text}>{text}</span>;
  }
  const { head, tail } = split;
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
          minWidth: MIN_HEAD,
        }}
      >
        {head}
      </span>
      <span style={{ whiteSpace: "nowrap", flexShrink: 0 }}>{tail}</span>
    </span>
  );
}
