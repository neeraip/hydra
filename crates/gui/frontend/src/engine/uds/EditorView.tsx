/** The uds implementation of the Editor view: a read-only composition
 * browser until urban-drainage editing lands. Element vocabulary is this
 * engine's own — a bespoke component may know its kinds by name. */

import { TypeBadge } from "../../components/ui/TypeBadge";
import { useLinks, useNodes, useRegions } from "../../hooks";

const KIND_LABELS: Record<string, string> = {
  junction: "Junctions",
  outfall: "Outfalls",
  divider: "Dividers",
  storage: "Storage units",
  raingage: "Rain gages",
  conduit: "Conduits",
  pump: "Pumps",
  orifice: "Orifices",
  weir: "Weirs",
  outlet: "Outlets",
  subcatchment: "Subcatchments",
};

function countByKind(items: Array<{ type: string }>): Array<[string, number]> {
  const counts = new Map<string, number>();
  for (const item of items) {
    counts.set(item.type, (counts.get(item.type) ?? 0) + 1);
  }
  return [...counts.entries()];
}

function KindGroup({
  title,
  entries,
}: {
  title: string;
  entries: Array<[string, number]>;
}) {
  if (entries.length === 0) return null;
  return (
    <div
      style={{
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius: 6,
        padding: "10px 12px",
      }}
    >
      <div
        style={{
          fontSize: "var(--text-sm)",
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: "0.05em",
          marginBottom: 8,
        }}
      >
        {title}
      </div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fill, minmax(160px, 1fr))",
          gap: "6px 14px",
        }}
      >
        {entries.map(([kind, count]) => {
          return (
            <div
              key={kind}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                fontSize: "var(--text-md)",
              }}
            >
              <TypeBadge type={kind} />
              <span style={{ color: "var(--text-primary)" }}>
                {KIND_LABELS[kind] ?? kind}
              </span>
              <span
                style={{
                  marginLeft: "auto",
                  color: "var(--text-secondary)",
                  fontFamily: "var(--font-mono)",
                }}
              >
                {count.toLocaleString()}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

export function UdsEditorView() {
  const nodes = useNodes();
  const links = useLinks();
  const regions = useRegions();

  return (
    <div
      style={{
        flex: 1,
        overflowY: "auto",
        padding: 18,
        display: "flex",
        flexDirection: "column",
        gap: 12,
      }}
    >
      <KindGroup title="Subcatchments" entries={countByKind(regions)} />
      <KindGroup title="Nodes" entries={countByKind(nodes)} />
      <KindGroup title="Links" entries={countByKind(links)} />
      <div
        style={{
          fontSize: "var(--text-sm)",
          color: "var(--text-tertiary)",
          lineHeight: 1.5,
        }}
      >
        Read-only — editing Urban Drainage models is not available in the GUI
        yet. Browse elements in the Network list, or edit the model file
        directly and re-import it.
      </div>
    </div>
  );
}
