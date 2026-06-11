import type { ScenarioDto } from "../../../hooks";

export interface FlatScenario extends ScenarioDto {
  depth: number;
}

/** BFS flatten preserving parent→child depth for indentation. */
export function flattenScenarios(dtos: ScenarioDto[]): FlatScenario[] {
  const byId = new Map(dtos.map((d) => [d.id, d]));
  const childrenOf = new Map<string | null, ScenarioDto[]>();
  for (const d of dtos) {
    const key = d.parentScenarioId ?? null;
    if (!childrenOf.has(key)) childrenOf.set(key, []);
    childrenOf.get(key)?.push(d);
  }
  const result: FlatScenario[] = [];
  const queue: Array<{ id: string; depth: number }> = (
    childrenOf.get(null) ?? []
  ).map((d) => ({
    id: d.id,
    depth: 0,
  }));
  while (queue.length > 0) {
    const { id, depth } = queue.shift()!;
    const dto = byId.get(id);
    if (!dto) continue;
    result.push({ ...dto, depth });
    for (const child of childrenOf.get(id) ?? []) {
      queue.push({ id: child.id, depth: depth + 1 });
    }
  }
  return result;
}

export const STATE_LABEL: Record<string, string> = {
  "not-run": "Not run",
  running: "Running…",
  simulated: "Simulated",
  failed: "Failed",
};

export const STATE_COLOR: Record<string, string> = {
  "not-run": "var(--text-tertiary)",
  running: "#d9aa57",
  simulated: "#7bbf95",
  failed: "#d97b7b",
};

export const rowButtonStyle: React.CSSProperties = {
  fontSize: 11,
  fontWeight: 500,
  padding: "3px 9px",
  border: "1px solid var(--border-hover)",
  borderRadius: 5,
  background: "transparent",
  color: "var(--text-secondary)",
  cursor: "pointer",
  fontFamily: "var(--font-ui)",
  whiteSpace: "nowrap",
};

export const iconButtonStyle: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  justifyContent: "center",
  width: 26,
  height: 26,
  border: "none",
  borderRadius: 5,
  background: "transparent",
  color: "var(--text-tertiary)",
  cursor: "pointer",
  padding: 0,
  transition: "background 0.1s, color 0.1s",
};
