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

import { useEffect, useMemo, useState } from "react";
import { useActiveProject } from "../../AppContext";
import type { CreateNodeModalProps } from "../../engine/registry";
import {
  createElement,
  useElementAttributes,
  useElementKinds,
} from "../../hooks";
import { useUnitSystem } from "../../units";
import {
  CreateElementDialog,
  type CreateKind,
  CreateNumberField,
} from "./CreateElementDialog";

export function CreateElementModal({
  open,
  suggestId,
  position,
  onCreated,
  onCancel,
}: CreateNodeModalProps) {
  const { project } = useActiveProject();
  const projectId = project?.id ?? "";
  const engine = project?.engine;
  const sys = useUnitSystem();

  // The kinds this dialog offers: creatable, and of a class a position
  // places. A polyline is drawn between two elements, which is a gesture
  // the map has and this dialog does not.
  const catalog = useElementKinds(engine);
  const kinds: CreateKind[] = useMemo(
    () =>
      catalog
        .filter(
          (k) => k.creatable && (k.class === "point" || k.class === "region"),
        )
        .map((k) => ({ value: k.id, label: k.label })),
    [catalog],
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

  const schema = useElementAttributes(engine, kind);
  // Only the numbers. A new element's references and choices keep the
  // engine's defaults and are changed afterwards in the table, where
  // they have their proper editors and the model to resolve against.
  const fields = useMemo(
    () =>
      schema.filter(
        (a) =>
          a.editable &&
          (a.kind?.type === "number" || a.kind?.type === "integer"),
      ),
    [schema],
  );

  const first = kinds[0]?.value ?? "";
  useEffect(() => {
    if (!open) return;
    setKind(first);
    setId(first ? suggestId(first) : "");
    setEdited(false);
    setValues({});
    setTypedAt([0, 0]);
  }, [open, first, suggestId]);

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
          position: position ?? typedAt,
          fields: values,
        });
        onCreated(kind, name);
      }}
      onCancel={onCancel}
    >
      {fields.map((a) => (
        <CreateNumberField
          key={a.key}
          label={a.label}
          value={values[a.key] ?? 0}
          quantity={a.quantity}
          sys={sys}
          onCommit={(v) => setValues((prev) => ({ ...prev, [a.key]: v }))}
        />
      ))}
      {/* Only when the caller could not say: a click on the map already
          answered this, and asking again would invite disagreeing with
          it. The numbers are the model's own coordinate system, which is
          why they carry no quantity — a model may be a map or a drawing
          and this dialog cannot tell. */}
      {!position && (
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
