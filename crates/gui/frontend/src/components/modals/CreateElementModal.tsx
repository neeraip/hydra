// ── Adding an element ────────────────────────────────────────────────────────
//
// Every part of this comes from the engine's catalogs. Which kinds are
// offered is §4.2's `creatable`, so a storage unit that needs a
// stage-area relation is simply absent rather than present and refusing
// on submit — which would teach the user the same thing one click later.
// Which fields are asked for is §4.4's schema, so a form and a table
// cannot come to disagree about what a junction's first number is
// called, or about its unit.
//
// It was the drainage node dialog, hardcoded to two kinds and one
// field. That is why the water-distribution Add button did nothing: the
// button asked the catalog whether the kind could be created and the
// catalog said yes, while the dialog behind it only existed for one
// engine.

import { useEffect, useId, useMemo, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import type { CreateNodeModalProps } from "../../engine/registry";
import {
  createElement,
  useElementAttributes,
  useElementKinds,
  useReferenceIds,
} from "../../hooks";
import { compareNatural } from "../../naturalOrder";
import { useUnitSystem } from "../../units";
import { offerDatalist } from "../panels/editorTable";
import {
  CreateElementDialog,
  type CreateKind,
  CreateNumberField,
} from "./CreateElementDialog";

export function CreateElementModal({
  open,
  suggestId,
  position,
  klass = "point",
  kind: preferKind,
  fromNodeId,
  toNodeId,
  spanLength,
  onCreated,
  onCancel,
}: CreateNodeModalProps) {
  const { project } = useActiveProject();
  const { activeScenarioId: scenarioId } = useAppState();
  const projectId = project?.id ?? "";
  const engine = project?.engine;
  const sys = useUnitSystem();

  // The creatable kinds of the class being added. A point or a region
  // is placed by a position; a polyline by naming its two ends.
  const catalog = useElementKinds(engine);
  const kinds: CreateKind[] = useMemo(
    () =>
      catalog
        .filter((k) => k.creatable && k.class === klass)
        .map((k) => ({ value: k.id, label: k.label })),
    [catalog, klass],
  );

  // The ids a polyline may name: every point the model has, whatever
  // kind of point. Fetched only for that case.
  const pointKinds = useMemo(
    () =>
      klass === "polyline"
        ? catalog.filter((k) => k.class === "point").map((k) => k.id)
        : [],
    [catalog, klass],
  );
  const endIds = useReferenceIds(project?.id, scenarioId, pointKinds);
  const allEnds = useMemo(
    () => Object.values(endIds).flat().sort(compareNatural),
    [endIds],
  );

  const [kind, setKind] = useState("");
  const [id, setId] = useState("");
  const [edited, setEdited] = useState(false);
  const [values, setValues] = useState<Record<string, number>>({});
  // Where to put it, when nobody pointed at anywhere. A dialog opened
  // from a table has no click behind it, and an element with no position
  // is one the map cannot draw — so the position is asked for rather
  // than defaulted to the origin, which is a real place and would be a
  // lie about where the element is.
  const [typedAt, setTypedAt] = useState<[number, number]>([0, 0]);
  // The ends, when the caller could not name them — an add from the
  // table has no drawn line behind it.
  const [typedFrom, setTypedFrom] = useState("");
  const [typedTo, setTypedTo] = useState("");

  // What a field starts at before anyone types in it. Only one has an
  // answer: a link drawn across a known distance starts its length
  // there, which is the one number the gesture itself measured. Not
  // stored into `values` — a seed that lived in state would have to be
  // re-seeded on every kind change, and would be indistinguishable from
  // a number the user typed.
  const schemaFields = useElementAttributes(engine, kind);

  const seeded = useMemo<Record<string, number>>(() => {
    const at: Record<string, number> = {};
    if (spanLength != null) at.length = spanLength;
    return at;
  }, [spanLength]);

  // What the engine says a field starts at, when it says anything.
  //
  // Zero is the wrong opening value for the fields that have a
  // conventional one — a weir created with a discharge coefficient of
  // nought passes no flow — and the catalog already had somewhere to put
  // it, so the answer is the engine's rather than this form's.
  const declared = useMemo<Record<string, number>>(() => {
    const at: Record<string, number> = {};
    for (const a of schemaFields) {
      const d = a.kind?.type === "number" ? a.kind.default : null;
      if (typeof d === "number") at[a.key] = d;
    }
    return at;
  }, [schemaFields]);

  // Numbers, and the attributes that name another element.
  //
  // The references are the reason half this engine's kinds could not be
  // created. A subcatchment must name a rain gage and an outlet, and the
  // model holds both as indices with no value meaning "not yet chosen" —
  // so they cannot be left to the edit that follows the create, the way a
  // number can. This dialog asking for them is what makes the kind
  // creatable at all.
  //
  // Choices are asked for too. The reasoning that once excluded them —
  // a choice always *has* a value, so a create can express it by leaving
  // it alone — holds for an element that already exists and fails for
  // one being built: a drainage inlet's grate family decides what the
  // engine constructs, not merely what it is set to afterwards. Each
  // starts at the default the catalog declares.
  const fields = useMemo(
    () =>
      schemaFields.filter(
        (a) =>
          a.editable &&
          (a.kind?.type === "number" ||
            a.kind?.type === "integer" ||
            a.kind?.type === "choice" ||
            a.kind?.type === "boolean" ||
            (a.references?.length ?? 0) > 0),
      ),
    [schemaFields],
  );

  // The ids each reference field may name, for the kinds those fields
  // declare (§4.5.1.1).
  const referenced = useMemo(
    () => [...new Set(fields.flatMap((a) => a.references ?? []))],
    [fields],
  );
  const referenceIds = useReferenceIds(project?.id, scenarioId, referenced);

  // What the user has typed or picked in the textual fields, by key —
  // references and choices alike, since both send a string. Separate from
  // `values`, which holds numbers.
  const [named, setNamed] = useState<Record<string, string>>({});

  // A choice's declared default, which is what it starts at. Unlike a
  // number there is no sensible zero to fall back to: the first item is
  // the engine's own answer only if it said so.
  const chosen = useMemo<Record<string, string>>(() => {
    const at: Record<string, string> = {};
    for (const a of fields) {
      if (a.kind?.type === "choice") {
        at[a.key] = a.kind.default ?? a.kind.items[0]?.value ?? "";
      } else if (a.kind?.type === "boolean") {
        at[a.key] = a.kind.default ? "Yes" : "No";
      }
    }
    return at;
  }, [fields]);

  // The kind the caller was looking at, when it is one this dialog is
  // offering. Pressing Add on the weirs table and being handed a conduit
  // is the dialog answering a question nobody asked — the table already
  // said which kind, and the first of the class is only the right answer
  // when nothing else has been said.
  const opensOn = useMemo(
    () =>
      kinds.find((k) => k.value === preferKind)?.value ?? kinds[0]?.value ?? "",
    [kinds, preferKind],
  );
  useEffect(() => {
    if (!open) return;
    setKind(opensOn);
    setId(opensOn ? suggestId(opensOn) : "");
    setEdited(false);
    setValues({});
    setTypedAt([0, 0]);
    setTypedFrom(fromNodeId ?? "");
    setTypedTo(toNodeId ?? "");
    setNamed({});
  }, [open, opensOn, suggestId, fromNodeId, toNodeId]);

  return (
    <CreateElementDialog
      open={open && kinds.length > 0}
      title="Add element"
      kinds={kinds}
      kind={kind}
      onKindChange={(next) => {
        setKind(next);
        // Values are per kind, and a maximum depth typed for a junction
        // means nothing to an outfall. Cleared rather than carried.
        setValues({});
        setNamed({});
        // A suggested id follows the kind until the user takes it over —
        // typing a name and having it replaced on the next click is the
        // behaviour this avoids.
        if (!edited) setId(suggestId(next));
      }}
      id={id}
      onIdChange={(next) => {
        setEdited(true);
        setId(next);
      }}
      onSubmit={async () => {
        const name = id.trim();
        await createElement(projectId, {
          kind,
          id: name,
          ...(klass === "polyline"
            ? { fromId: typedFrom.trim(), toId: typedTo.trim() }
            : klass === "collection"
              ? // Nowhere to be and nothing to run between: a container
                // is its name and its contents, and the contents are
                // edited in the panel below the table.
                {}
              : { position: position ?? typedAt }),
          fields: { ...declared, ...chosen, ...seeded, ...values, ...named },
        });
        onCreated(kind, name);
      }}
      onCancel={onCancel}
    >
      {fields.map((a) =>
        a.kind?.type === "choice" || a.kind?.type === "boolean" ? (
          <ChoiceRow
            key={a.key}
            label={a.label}
            value={named[a.key] ?? chosen[a.key] ?? ""}
            items={
              a.kind.type === "choice"
                ? a.kind.items
                : [
                    { value: "Yes", label: "Yes" },
                    { value: "No", label: "No" },
                  ]
            }
            onChange={(v) => setNamed((prev) => ({ ...prev, [a.key]: v }))}
          />
        ) : a.references?.length ? (
          <ReferenceField
            key={a.key}
            label={a.label}
            value={named[a.key] ?? ""}
            options={[
              ...new Set(a.references.flatMap((k) => referenceIds[k] ?? [])),
            ].sort(compareNatural)}
            onChange={(v) => setNamed((prev) => ({ ...prev, [a.key]: v }))}
          />
        ) : (
          <CreateNumberField
            key={a.key}
            label={a.label}
            value={values[a.key] ?? seeded[a.key] ?? declared[a.key] ?? 0}
            quantity={a.quantity}
            sys={sys}
            onCommit={(v) => setValues((prev) => ({ ...prev, [a.key]: v }))}
          />
        ),
      )}
      {klass === "polyline" && (
        <>
          <ReferenceField
            label="From"
            value={typedFrom}
            options={allEnds}
            readOnly={fromNodeId != null}
            onChange={setTypedFrom}
          />
          <ReferenceField
            label="To"
            value={typedTo}
            options={allEnds}
            readOnly={toNodeId != null}
            onChange={setTypedTo}
          />
        </>
      )}
      {/* Only when the caller could not say: a click on the map already
          answered this, and asking again would invite disagreeing with
          it. The numbers are the model's own coordinate system, which is
          why they carry no quantity — a model may be a map or a drawing
          and this dialog cannot tell. */}
      {klass !== "polyline" && klass !== "collection" && !position && (
        <>
          <CreateNumberField
            label="X"
            value={typedAt[0]}
            sys={sys}
            onCommit={(v) => setTypedAt(([, y]) => [v, y])}
          />
          <CreateNumberField
            label="Y"
            value={typedAt[1]}
            sys={sys}
            onCommit={(v) => setTypedAt(([x]) => [x, v])}
          />
        </>
      )}
    </CreateElementDialog>
  );
}

/** One of a declared list, in the dialog's own field chrome. */
function ChoiceRow({
  label,
  value,
  items,
  onChange,
}: {
  label: string;
  value: string;
  items: Array<{ value: string; label: string }>;
  onChange: (value: string) => void;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <span
        style={{
          fontSize: "var(--text-sm)",
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: "0.06em",
        }}
      >
        {label}
      </span>
      <select
        aria-label={label}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        style={{
          background: "var(--bg-input)",
          border: "1px solid var(--border)",
          borderRadius: 6,
          padding: "6px 10px",
          color: "var(--text-primary)",
          fontFamily: "var(--font-mono)",
          fontSize: "var(--text-md)",
          outline: "none",
        }}
      >
        {items.map((i) => (
          <option key={i.value} value={i.value}>
            {i.label}
          </option>
        ))}
      </select>
    </div>
  );
}

/**
 * An end of a link: typed, with the model's own ids offered.
 *
 * Read-only when the caller already named it — a line drawn between two
 * elements on the map has answered this, and a field that let it be
 * changed would invite disagreeing with the line on screen.
 */
function ReferenceField({
  label,
  value,
  options,
  readOnly,
  onChange,
}: {
  label: string;
  value: string;
  options: string[];
  readOnly?: boolean;
  onChange: (value: string) => void;
}) {
  const listId = useId();
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
      <span
        style={{
          fontSize: "var(--text-sm)",
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: "0.06em",
        }}
      >
        {label}
      </span>
      {offerDatalist(options.length) && (
        <datalist id={listId}>
          {options.map((o) => (
            <option key={o} value={o} />
          ))}
        </datalist>
      )}
      <input
        aria-label={label}
        value={value}
        readOnly={readOnly}
        list={offerDatalist(options.length) ? listId : undefined}
        onChange={(e) => onChange(e.target.value)}
        style={{
          background: readOnly ? "transparent" : "var(--bg-input)",
          border: `1px solid ${readOnly ? "transparent" : "var(--border)"}`,
          borderRadius: 6,
          padding: "6px 10px",
          color: "var(--text-primary)",
          fontFamily: "var(--font-mono)",
          fontSize: "var(--text-md)",
          outline: "none",
        }}
      />
    </div>
  );
}
