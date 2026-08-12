// ── Adding a drainage link ───────────────────────────────────────────────────
//
// Conduits only. A pump needs a characteristic curve, an outlet a rating,
// and an orifice or weir an opening geometry — none of which can be
// defaulted, so none of them is offered here.
//
// Two numbers: the length, which the canvas already knows because it
// drew the line, and the diameter, which it cannot know. The length is
// offered rather than assumed, because a drawn line is a plan distance
// and a conduit's length is a pipe — they differ on any slope, and by a
// lot in a manhole drop.

import { useEffect, useMemo, useState } from "react";
import { useActiveProject } from "../../AppContext";
import {
  CreateElementDialog,
  type CreateKind,
  CreateNumberField,
} from "../../components/modals/CreateElementDialog";
import { createElement, useElementAttributes } from "../../hooks";
import { useUnitSystem } from "../../units";
import type { CreateLinkModalProps } from "../registry";

const KINDS: CreateKind[] = [{ value: "conduit", label: "Conduit" }];

/** 300 mm — the smallest pipe most standards allow in a public sewer,
 * and the size a modeller is least surprised to have to change. */
const DEFAULT_DIAMETER_M = 0.3;

export function UdsCreateLinkModal({
  open,
  suggestId,
  fromNodeId,
  toNodeId,
  spanLength,
  onCreated,
  onCancel,
}: CreateLinkModalProps) {
  const { project } = useActiveProject();
  const projectId = project?.id ?? "";
  const sys = useUnitSystem();
  const [id, setId] = useState("");
  const [length, setLength] = useState(0);
  const [diameter, setDiameter] = useState(DEFAULT_DIAMETER_M);

  const schema = useElementAttributes("uds", "conduit");
  const lengthAttr = useMemo(
    () => schema.find((a) => a.key === "length"),
    [schema],
  );

  useEffect(() => {
    if (!open) return;
    setId(suggestId("conduit"));
    // The drawn distance, as a starting point rather than an answer.
    // Zero when the canvas cannot supply one — a plan model's
    // coordinates are in the file's own unit, and a guess there is a
    // length out by a factor of three on every US-unit model. The
    // backend refuses a zero length, so an untouched field cannot
    // become a conduit of no length.
    setLength(spanLength ?? 0);
    setDiameter(DEFAULT_DIAMETER_M);
  }, [open, suggestId, spanLength]);

  const quantity = lengthAttr?.quantity;

  return (
    <CreateElementDialog
      open={open}
      title="Add link"
      kinds={KINDS}
      kind="conduit"
      onKindChange={() => {}}
      id={id}
      onIdChange={setId}
      idPlaceholder="e.g. C1"
      note={
        spanLength == null
          ? `From ${fromNodeId} to ${toNodeId}. This model's coordinates carry no scale, so the length is yours to give. Circular, Manning n 0.013, both ends flush with their inverts.`
          : `From ${fromNodeId} to ${toNodeId}. Circular, Manning n 0.013, both ends flush with their inverts.`
      }
      onSubmit={async () => {
        const name = id.trim();
        await createElement(projectId, {
          kind: "conduit",
          id: name,
          fromId: fromNodeId,
          toId: toNodeId,
          fields: { length, diameter },
        });
        onCreated("conduit", name);
      }}
      onCancel={onCancel}
    >
      <CreateNumberField
        label="Length"
        value={length}
        quantity={quantity}
        sys={sys}
        onCommit={setLength}
      />
      <CreateNumberField
        label="Diameter"
        value={diameter}
        quantity={quantity}
        sys={sys}
        onCommit={setDiameter}
      />
    </CreateElementDialog>
  );
}
