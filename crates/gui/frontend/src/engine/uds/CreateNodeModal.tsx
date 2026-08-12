// ── Adding a drainage node ───────────────────────────────────────────────────
//
// The dialog chrome is the app's; what is drainage's own is which kinds
// can be added and what a new one needs. Storage units and dividers are
// absent rather than disabled: a storage unit needs a stage-area relation
// and a divider needs to be told which link its flow leaves by, and
// neither can be invented. Offering them and refusing on submit would
// teach the user the same thing one click later.
//
// The one field is the invert, and its label and unit are read from the
// engine's own §4.4 schema rather than written here — the same source the
// inspector and the element tables use, so a form and a table cannot come
// to disagree about what a junction's first number is called.

import { useEffect, useMemo, useState } from "react";
import { useActiveProject } from "../../AppContext";
import {
  CreateElementDialog,
  type CreateKind,
  CreateNumberField,
} from "../../components/modals/CreateElementDialog";
import { createElement, useElementAttributes } from "../../hooks";
import { useUnitSystem } from "../../units";
import type { CreateNodeModalProps } from "../registry";

const KINDS: CreateKind[] = [
  { value: "junction", label: "Junction" },
  { value: "outfall", label: "Outfall" },
];

export function UdsCreateNodeModal({
  open,
  suggestId,
  position,
  onCreated,
  onCancel,
}: CreateNodeModalProps) {
  const { project } = useActiveProject();
  const projectId = project?.id ?? "";
  const sys = useUnitSystem();
  const [kind, setKind] = useState("junction");
  const [id, setId] = useState("");
  const [invert, setInvert] = useState(0);
  const [edited, setEdited] = useState(false);

  const schema = useElementAttributes("uds", kind);
  const invertAttr = useMemo(
    () => schema.find((a) => a.key === "invert"),
    [schema],
  );

  useEffect(() => {
    if (!open) return;
    setKind("junction");
    setId(suggestId("junction"));
    setEdited(false);
    setInvert(0);
  }, [open, suggestId]);

  const label = invertAttr?.label ?? "Invert elevation";
  const quantity = invertAttr?.quantity;
  return (
    <CreateElementDialog
      open={open}
      title="Add node"
      kinds={KINDS}
      kind={kind}
      onKindChange={(next) => {
        setKind(next);
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
      idPlaceholder={kind === "outfall" ? "e.g. O1" : "e.g. J1"}
      note={
        kind === "junction"
          ? "Its rim rises to the crown of the highest conduit reaching it."
          : "A free outfall: the stage follows the connecting conduit."
      }
      onSubmit={async () => {
        if (!position) throw new Error("no position for the new node");
        const name = id.trim();
        await createElement(projectId, {
          kind,
          id: name,
          position,
          fields: { invert },
        });
        onCreated(kind, name);
      }}
      onCancel={onCancel}
    >
      <CreateNumberField
        label={label}
        value={invert}
        quantity={quantity}
        sys={sys}
        onCommit={setInvert}
      />
    </CreateElementDialog>
  );
}
