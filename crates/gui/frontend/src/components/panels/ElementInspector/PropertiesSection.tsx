// ── The Properties section of the element inspector ──────────────────────────
//
// The §4.4 attribute rows for one element, in the presentation both
// engines' inspector bodies use — because there is no reason for them to
// differ, and one of them was drifting.
//
// It was under `engine/uds/`, serving the drainage bodies alone. The
// water-distribution body wrote its own rows out of the network snapshot
// instead: hardcoded labels, hardcoded units, and read-only. So the same
// junction offered every property for editing in the Editor's table and
// none of them in the inspector — one value with two answers about
// itself, which is the shape of defect this codebase keeps finding.
//
// What a row offers comes from `cellEditor`, the same decision the
// table's cells make. The one before it, `editableNumberOf`, said "a
// number, and text is never editable" — true while only numbers were
// writable, and quietly wrong the day a tag and an outlet became text
// that is.

import type React from "react";
import { useCallback, useEffect, useId, useMemo, useState } from "react";
import { useActiveProject, useAppState } from "../../../AppContext";
import {
  type ElementAttribute,
  type ElementAttributeInfo,
  formatElementAttribute,
  getElementDetails,
  useElementAttributes,
  useReferenceIds,
} from "../../../hooks";
import { useElementAttributeWrite } from "../../../hooks/useAttributeWrite";
import { useUnitSystem } from "../../../units";
import { SectionLabel } from "../../ui/SectionLabel";
import { AttributeField } from "../attributeField";
import { cellEditor } from "../cellEditor";
import { offerDatalist } from "../editorTable";
import { PropRow } from "./primitives";
import { RecordSets, useElementRecords } from "./RecordSets";

/** Fetch the engine-described attribute rows for one element. */
/** Rows already fetched, so re-selecting an element does not go blank
 * while the same answer is fetched again. */
const detailCache = new Map<string, ElementAttribute[]>();

/**
 * A element's §4.4 property rows, and how much space to leave for them.
 *
 * Properties arrive over IPC, and the node, link and region bodies are
 * separate components — so selecting a junction after a catchment mounts a
 * fresh body whose rows are null for one round trip. Rendering nothing in
 * that gap collapsed the section, then restored it, shoving everything
 * below it down the panel.
 *
 * The schema is the answer to that: a kind's properties are declared, so
 * their names are known before any element is fetched. The section draws
 * its real rows immediately and fills the values in when they land.
 */
export function useElementDetails(
  elementId: string,
  kind?: string,
): {
  rows: ElementAttribute[] | null;
  schema: ElementAttributeInfo[];
  elementId: string;
  onEdited: () => void;
} {
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();
  const schema = useElementAttributes(project?.engine, kind);
  const key = `${project?.id ?? ""}\u0000${activeScenarioId ?? ""}\u0000${elementId}`;
  const [rows, setRows] = useState<ElementAttribute[] | null>(
    () => detailCache.get(key) ?? null,
  );
  // After a write, fetch what the model now holds and replace the cached
  // answer. Refetching directly rather than through a counter that only
  // exists to re-run the effect: the cache is there so re-selecting an
  // element is instant, not so an edit is invisible.
  const onEdited = useCallback(() => {
    if (!project?.id) return;
    getElementDetails(project.id, activeScenarioId, elementId).then((r) => {
      if (r) detailCache.set(key, r);
      setRows(r);
    });
  }, [project?.id, activeScenarioId, elementId, key]);
  useEffect(() => {
    if (!project?.id) return;
    const cached = detailCache.get(key);
    if (cached) {
      setRows(cached);
      return;
    }
    setRows(null);
    let cancelled = false;
    getElementDetails(project.id, activeScenarioId, elementId).then((r) => {
      if (r) detailCache.set(key, r);
      if (!cancelled) setRows(r);
    });
    return () => {
      cancelled = true;
    };
  }, [project?.id, activeScenarioId, elementId, key]);
  return { rows, schema, elementId, onEdited };
}

/** Properties section: §4 schema rows in the wds table presentation. */
export function PropertiesSection({
  rows,
  schema = [],
  elementId,
  onEdited,
  children,
}: {
  rows: ElementAttribute[] | null;
  /** The kind's declared properties, drawn while the values load. */
  schema?: ElementAttributeInfo[];
  /** The element these rows belong to. Absent = the section reads only,
   * which is what a caller with no element to address should get. */
  elementId?: string;
  /** Called after a successful write, so the caller can refetch. */
  onEdited?: () => void;
  /** Extra rows appended inside the table — a preview, a shortcut, a
   * chart. A body's own material, so this component never learns what
   * it is. */
  children?: React.ReactNode;
}) {
  const sys = useUnitSystem();
  const { project } = useActiveProject();
  const { activeScenarioId } = useAppState();

  // The ids a reference row may name, fetched for the kinds its rows
  // declare (§4.5.1.1) and no others — which for most elements is none,
  // so most inspections fetch nothing.
  const referenced = useMemo(
    () => [...new Set((rows ?? []).flatMap((r) => r.references ?? []))],
    [rows],
  );
  const referenceIds = useReferenceIds(
    project?.id,
    activeScenarioId,
    referenced,
  );
  const listPrefix = useId();
  const lists = useMemo(() => {
    const out: Array<{ key: string; ids: string[] }> = [];
    for (const r of rows ?? []) {
      // The union across every kind the row may name, sorted so it reads
      // as one set of ids rather than as several lists run together.
      const ids = [
        ...new Set((r.references ?? []).flatMap((k) => referenceIds[k] ?? [])),
      ].sort();
      // Above the cutoff the list is dropped rather than truncated: a
      // shortened list silently hides valid ids while still looking
      // authoritative.
      if (ids.length && offerDatalist(ids.length))
        out.push({ key: r.key, ids });
    }
    return out;
  }, [rows, referenceIds]);

  // Nothing known yet, but this kind has been seen before: hold the height
  // rather than collapsing and shoving the rest of the panel about.
  // Labels are declared, values are fetched. Draw what is known and leave
  // the values blank for the moment rather than drawing nothing at all.
  if (!rows && schema.length > 0) {
    return (
      <>
        <SectionLabel>Properties</SectionLabel>
        <table
          style={{
            width: "100%",
            borderCollapse: "collapse",
            marginBottom: 14,
          }}
        >
          <tbody>
            {schema.map((a) => (
              <PropRow key={a.key} label={a.label} value="—" />
            ))}
            {children}
          </tbody>
        </table>
      </>
    );
  }
  if (!rows || rows.length === 0) return null;
  return (
    <>
      <SectionLabel>Properties</SectionLabel>
      {lists.map((l) => (
        <datalist key={l.key} id={`${listPrefix}-${l.key}`}>
          {l.ids.map((id) => (
            <option key={id} value={id} />
          ))}
        </datalist>
      ))}
      <table
        style={{ width: "100%", borderCollapse: "collapse", marginBottom: 14 }}
      >
        <tbody>
          {rows.map((r) =>
            elementId ? (
              <AttrRow
                key={r.key}
                attr={r}
                sys={sys}
                elementId={elementId}
                listId={
                  lists.some((l) => l.key === r.key)
                    ? `${listPrefix}-${r.key}`
                    : undefined
                }
                onEdited={onEdited}
              />
            ) : (
              <PropRow
                key={r.key}
                label={r.label}
                value={formatElementAttribute(r, sys)}
              />
            ),
          )}
          {children}
        </tbody>
      </table>
      {/* What the element *has*, after what it *is*. A junction's demand
          categories sit under its properties because that is where a
          reader looks for them, and because the attribute rows above can
          only ever show their sum (§4.5.2.3). */}
      {elementId && <ElementRecords elementId={elementId} />}
    </>
  );
}

/** The record sets attached to this element, fetched here so a caller
 *  passing rows does not also have to know about them. */
function ElementRecords({ elementId }: { elementId: string }) {
  const { sets, refetch } = useElementRecords(elementId);
  return <RecordSets elementId={elementId} sets={sets} onEdited={refetch} />;
}

/**
 * One Properties row, offering whatever its declared shape can hold.
 *
 * The decision is `cellEditor`'s, which is the table's — so a property
 * that takes an input in one surface takes one in the other, and the
 * pair cannot drift again without the shared function changing under
 * both.
 *
 * What this row adds is where the field sits: in `PropRow`'s grid, so an
 * editable row lines up with the read-only rows above and below it
 * rather than reading as a second table, and with the unit beside it,
 * labelling rather than participating.
 */
function AttrRow({
  attr,
  sys,
  elementId,
  listId,
  onEdited,
}: {
  attr: ElementAttribute;
  sys: "si" | "us";
  elementId: string;
  /** The datalist of ids this row may name, when it is a reference. */
  listId?: string;
  onEdited?: () => void;
}) {
  const write = useElementAttributeWrite();
  const q = attr.quantity;
  // A row carries one of the two, so the shown value is whichever it
  // has — the same value the write's undo restores.
  const shown = attr.number ?? attr.text ?? null;
  const editor = cellEditor(
    { ...attr, values: [] },
    shown,
    /* listening */ true,
  );
  if (editor.kind === "none") {
    return (
      <PropRow label={attr.label} value={formatElementAttribute(attr, sys)} />
    );
  }

  return (
    <tr>
      <td
        style={{
          fontSize: "var(--text-md)",
          color: "var(--text-tertiary)",
          padding: "4px 0",
          width: "45%",
        }}
      >
        {attr.label}
      </td>
      <td
        style={{
          fontSize: "var(--text-md)",
          padding: "4px 0",
          fontFamily: "var(--font-mono)",
          display: "flex",
          alignItems: "center",
          gap: 6,
        }}
      >
        <AttributeField
          editor={editor}
          quantity={q}
          sys={sys}
          label={attr.label}
          listId={listId}
          onCommit={(next) =>
            // The value the row was showing is what an undo restores.
            write(elementId, attr.key, next, shown ?? undefined).then(() =>
              onEdited?.(),
            )
          }
        />
        {q && (
          <span style={{ color: "var(--text-tertiary)" }}>
            {sys === "us" ? q.usLabel : q.siLabel}
          </span>
        )}
      </td>
    </tr>
  );
}
