import type { ScenarioDto } from "../../../hooks";

export interface FlatScenario extends ScenarioDto {
  depth: number;
}

/**
 * DFS flatten preserving parent→child adjacency: each scenario is followed
 * immediately by its descendants (depth tracks indentation). Mirrors the
 * ordering used by OverviewView's ScenarioList. Orphans whose parent isn't
 * present are appended at depth 0.
 */
export function flattenScenarios(dtos: ScenarioDto[]): FlatScenario[] {
  const childrenOf = new Map<string, ScenarioDto[]>();
  for (const d of dtos) {
    if (!d.parentScenarioId) continue;
    const arr = childrenOf.get(d.parentScenarioId) ?? [];
    arr.push(d);
    childrenOf.set(d.parentScenarioId, arr);
  }
  const result: FlatScenario[] = [];
  const walk = (dto: ScenarioDto, depth: number) => {
    result.push({ ...dto, depth });
    for (const child of childrenOf.get(dto.id) ?? []) walk(child, depth + 1);
  };
  for (const d of dtos) {
    if (!d.parentScenarioId) walk(d, 0);
  }
  // Append any orphans whose parent wasn't found.
  for (const d of dtos) {
    if (d.parentScenarioId && !dtos.some((p) => p.id === d.parentScenarioId)) {
      result.push({ ...d, depth: 0 });
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
