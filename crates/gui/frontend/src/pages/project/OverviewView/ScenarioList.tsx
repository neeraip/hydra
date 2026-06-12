import type { ScenarioDto } from "../../../hooks";
import { Dot, EmptyState } from "./primitives";

export function ScenarioList({
  scenarios,
  onOpen,
}: {
  scenarios: ScenarioDto[];
  onOpen: () => void;
}) {
  if (scenarios.length === 0) {
    return (
      <EmptyState
        message="Only the base model exists. Branch a scenario to test changes without disturbing the source."
        ctaLabel="Open scenarios"
        onCta={onOpen}
      />
    );
  }

  // Group by parent for a one-level hierarchy display.
  const roots = scenarios.filter((s) => !s.parentScenarioId);
  const childrenByParent = new Map<string, ScenarioDto[]>();
  for (const s of scenarios) {
    if (s.parentScenarioId) {
      const arr = childrenByParent.get(s.parentScenarioId) ?? [];
      arr.push(s);
      childrenByParent.set(s.parentScenarioId, arr);
    }
  }

  const rows: Array<{ s: ScenarioDto; depth: number }> = [];
  const walk = (s: ScenarioDto, depth: number) => {
    rows.push({ s, depth });
    for (const c of childrenByParent.get(s.id) ?? []) walk(c, depth + 1);
  };
  for (const r of roots) walk(r, 0);
  // Append any orphans whose parent wasn't found.
  for (const s of scenarios) {
    if (
      s.parentScenarioId &&
      !scenarios.some((p) => p.id === s.parentScenarioId)
    ) {
      rows.push({ s, depth: 0 });
    }
  }

  return (
    <div style={{ display: "flex", flexDirection: "column" }}>
      {rows.slice(0, 6).map(({ s, depth }, i) => (
        <ScenarioRow key={s.id} scenario={s} depth={depth} first={i === 0} />
      ))}
      {rows.length > 6 && (
        <button
          type="button"
          onClick={onOpen}
          onMouseEnter={(e) => {
            (e.currentTarget as HTMLButtonElement).style.opacity = "0.75";
          }}
          onMouseLeave={(e) => {
            (e.currentTarget as HTMLButtonElement).style.opacity = "1";
          }}
          style={{
            marginTop: 6,
            background: "transparent",
            border: "none",
            color: "var(--accent)",
            fontSize: 12,
            textAlign: "left",
            padding: "6px 0",
            cursor: "pointer",
            fontFamily: "var(--font-ui)",
            transition: "opacity var(--t-fast)",
          }}
        >
          +{rows.length - 6} more · View all scenarios →
        </button>
      )}
    </div>
  );
}

function ScenarioRow({
  scenario,
  depth,
  first,
}: {
  scenario: ScenarioDto;
  depth: number;
  first: boolean;
}) {
  const ran = scenario.state === "simulated";
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        padding: "8px 10px",
        paddingLeft: 10 + depth * 16,
        borderTop: first ? "none" : "1px solid var(--border)",
      }}
    >
      <Dot color={ran ? "var(--status-success)" : "var(--text-tertiary)"} />
      <span
        style={{
          fontSize: 13,
          color: "var(--text-primary)",
          flex: 1,
          minWidth: 0,
          overflow: "hidden",
          textOverflow: "ellipsis",
          whiteSpace: "nowrap",
        }}
      >
        {scenario.name}
      </span>
      <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
        {ran ? "simulated" : "not run"}
      </span>
    </div>
  );
}
