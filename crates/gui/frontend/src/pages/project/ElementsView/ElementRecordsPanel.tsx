/**
 * The records the selected element carries, below the Editor's table.
 *
 * The same slot a container's contents use, and for the same reason: a
 * junction's demand categories are a table, and a table does not fit in
 * a cell of another one.
 *
 * It exists because they showed in the canvas inspector and nowhere
 * here — one value giving two answers depending on which surface you
 * asked, which is the shape of defect this editor was rebuilt to remove.
 */

import {
  RecordSets,
  shownRecordSets,
  useElementRecords,
} from "../../../components/panels/ElementInspector/RecordSets";

export function ElementRecordsPanel({
  elementId,
  kind,
}: {
  elementId: string;
  /** Which kind the tab is showing — half the element's address in
   * water distribution. */
  kind?: string;
}) {
  const { sets, refetch } = useElementRecords(elementId, kind);
  // The same question the table below asks, asked here too: the strip is
  // a bordered box with a background, so it has to be absent rather than
  // empty when its one child would draw nothing.
  if (shownRecordSets(sets).length === 0) return null;
  return (
    <div
      style={{
        borderTop: "1px solid var(--border)",
        overflow: "auto",
        maxHeight: "45%",
        padding: "6px 12px",
        background: "var(--bg-panel)",
        flexShrink: 0,
      }}
    >
      <RecordSets
        elementId={elementId}
        kind={kind}
        sets={sets}
        onEdited={refetch}
      />
    </div>
  );
}
