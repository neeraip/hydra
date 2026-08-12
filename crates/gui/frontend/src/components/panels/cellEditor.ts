import type { KindColumn } from "../../hooks";

/**
 * What a table cell offers for editing, from the column's declared value
 * shape (hydra-common §3.2.1, reused by §4.4).
 *
 * A table that knew only "number or text" could offer a field and a
 * field only, so a valve type was a free-text box you could mistype and
 * a check valve was the word "Yes" you had to spell. The shape is
 * published; reading it is what lets one table render either engine's
 * columns without naming a single one of them.
 *
 * `none` is the answer for everything that cannot be edited in a cell:
 * a column the engine will not write, an element with no value for it,
 * and the list-valued shapes — a set of threshold edges is not a cell.
 */
export type CellEditor =
  | { kind: "none" }
  | { kind: "number"; value: number }
  | { kind: "text"; value: string; references?: string }
  | {
      kind: "choice";
      value: string;
      items: Array<{ value: string; label: string }>;
    };

export function cellEditor(
  column: KindColumn,
  value: number | string | null | undefined,
  listening: boolean,
): CellEditor {
  if (!listening || !column.editable || value == null) return { kind: "none" };
  switch (column.kind?.type) {
    case "number":
    case "integer":
      return typeof value === "number" && Number.isFinite(value)
        ? { kind: "number", value }
        : { kind: "none" };
    case "text":
      return typeof value === "string"
        ? { kind: "text", value }
        : { kind: "none" };
    case "boolean":
      // Rendered as a choice of two rather than a checkbox: the value
      // arrives as the engine's own word, and a checkbox would have to
      // invent which word means true.
      return typeof value === "string"
        ? {
            kind: "choice",
            value,
            items: [
              { value: "Yes", label: "Yes" },
              { value: "No", label: "No" },
            ],
          }
        : { kind: "none" };
    case "choice":
      return typeof value === "string"
        ? { kind: "choice", value, items: column.kind.items }
        : { kind: "none" };
    default:
      // numberList, multiChoice, and a column from an engine newer than
      // this build. Shown, never offered — a cell is one value.
      return { kind: "none" };
  }
}
