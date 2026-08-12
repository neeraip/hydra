// ── The input for one attribute value ────────────────────────────────────────
//
// One renderer for every surface that offers an attribute for editing:
// the Editor's per-kind tables and the canvas inspector's Properties
// rows. What it draws comes from the column's declared value shape
// (`cellEditor`, hydra-common §3.2.1 reused by §4.4) — a select for a
// valve type, a yes/no for a check valve, a field with the model's own
// ids for a reference, a numeric field for everything else.
//
// It exists because the two surfaces had drifted, and drifted the way
// this codebase drifts: one identifier answering two questions. The
// inspector decided editability with `editableNumberOf`, which said "a
// number, and text is never editable" — true when the only writable
// values were numbers, and quietly wrong once a tag and an outlet
// became writable. So the same attribute took an input in the table and
// read as fixed in the inspector, which is the same value giving two
// answers about itself.
//
// The only difference between the two surfaces is chrome: a cell shows
// no border until focused and fills its `<td>`; a row sits in the
// inspector's grid and looks like a field. That is a style, not a
// decision, so it is a prop.

import type React from "react";
import { useEffect, useState } from "react";
import type { ElementAttributeQuantity } from "../../hooks";
import { EditableNumber } from "../ui/EditableNumber";
import type { CellEditor } from "./cellEditor";

/** Where the field is being drawn. A cell fills its `<td>` and shows no
 *  chrome at rest; a row is a field in a form. */
export type FieldChrome = "cell" | "boxed";

const CELL_INPUT: React.CSSProperties = {
  display: "block",
  width: "100%",
  boxSizing: "border-box",
  padding: "7px 10px",
  background: "transparent",
  border: "none",
  outline: "none",
  borderRadius: 0,
  color: "var(--text-primary)",
  fontFamily: "var(--font-mono)",
  fontSize: "var(--text-md)",
};

const BOXED_INPUT: React.CSSProperties = {
  ...CELL_INPUT,
  padding: "3px 6px",
  background: "var(--bg-input)",
  border: "1px solid var(--border)",
  borderRadius: 4,
};

function inputStyle(chrome: FieldChrome): React.CSSProperties {
  return chrome === "cell" ? CELL_INPUT : BOXED_INPUT;
}

/**
 * The input for one attribute, or `null` where the value takes none.
 *
 * `null` is the answer for everything a field cannot hold: a column the
 * engine will not write, an element with no value for it, and the
 * list-valued shapes. A caller that gets `null` renders the value as
 * text — which is what it did before any of this was editable.
 */
export function AttributeField({
  editor,
  label,
  quantity,
  sys,
  chrome = "boxed",
  align,
  listId,
  onCommit,
}: {
  editor: CellEditor;
  /** Names the field for a screen reader. The row is identified by its
   *  element id, so callers pass "<id> <attribute>". */
  label: string;
  quantity?: ElementAttributeQuantity;
  sys: "si" | "us";
  chrome?: FieldChrome;
  align?: "left" | "right";
  /** The shared datalist of ids a reference may name, when the caller
   *  has one small enough to be worth offering. */
  listId?: string;
  onCommit: (value: number | string) => void;
}) {
  switch (editor.kind) {
    case "none":
      return null;
    case "number":
      return (
        <EditableNumber
          value={editor.value}
          quantity={quantity}
          sys={sys}
          label={label}
          chrome={chrome}
          align={align}
          onCommit={onCommit}
        />
      );
    case "choice":
      return (
        <ChoiceField
          label={label}
          value={editor.value}
          items={editor.items}
          chrome={chrome}
          onCommit={onCommit}
        />
      );
    case "text":
      return (
        <TextField
          label={label}
          value={editor.value}
          listId={listId}
          chrome={chrome}
          onCommit={onCommit}
        />
      );
  }
}

/** A value that is one of a declared list. */
function ChoiceField({
  label,
  value,
  items,
  chrome,
  onCommit,
}: {
  label: string;
  value: string;
  items: Array<{ value: string; label: string }>;
  chrome: FieldChrome;
  onCommit: (value: string) => void;
}) {
  return (
    <select
      aria-label={label}
      value={value}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => {
        if (e.target.value !== value) onCommit(e.target.value);
      }}
      style={{ ...inputStyle(chrome), cursor: "pointer" }}
    >
      {/* A value the engine holds that the list does not offer still has
          to be shown, or the select would silently claim the element is
          something it is not. */}
      {!items.some((i) => i.value === value) && (
        <option value={value}>{value}</option>
      )}
      {items.map((i) => (
        <option key={i.value} value={i.value}>
          {i.label}
        </option>
      ))}
    </select>
  );
}

/** A value that is free text — a reference to another element, most
 *  often. Committed on blur or Enter, abandoned on Escape, and silent
 *  when unchanged, exactly as the numeric field is. */
function TextField({
  label,
  value,
  listId,
  chrome,
  onCommit,
}: {
  label: string;
  value: string;
  listId?: string;
  chrome: FieldChrome;
  onCommit: (value: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <input
      aria-label={label}
      list={listId}
      value={draft}
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={() => {
        if (draft !== value) onCommit(draft);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter") e.currentTarget.blur();
        if (e.key === "Escape") {
          setDraft(value);
          e.currentTarget.blur();
        }
      }}
      style={inputStyle(chrome)}
    />
  );
}
