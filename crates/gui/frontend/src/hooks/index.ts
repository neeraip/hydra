/**
 * Data-source seam — barrel module.
 *
 * Every UI consumer reads project / network / scenario / task / time-series
 * data through this module. The implementation lives in domain modules
 * (`projects.ts`, `network.ts`, `scenarios.ts`, `queue.ts`, `simulation.ts`,
 * `results.ts`, `editors.ts`, `issues.ts`); this file only
 * re-exports so existing `import { ... } from "../hooks"` sites keep working.
 *
 * Rules for callers:
 *   - Never import from `../types` or `../projectConfig` directly — always
 *     go through this module so the seam stays in one place.
 *   - Treat returned arrays as referentially stable across renders for the
 *     same input (see the `useMemo` wrappers in the domain modules).
 */

export type { ProjectView } from "../projectConfig";
// Engine constants and view config.
export { ACCENT, LABEL, PILL, PROJECT_VIEWS } from "../projectConfig";
export type {
  Command,
  CommandCategory,
  Link,
  LinkType,
  Node,
  NodeType,
  Pattern,
  Task,
  TaskStatus,
} from "../types";
// Re-export pure helpers and constants — no hook needed.
export {
  deltaColor,
  PRESSURE_MAX,
  PRESSURE_MIN,
  PRESSURE_THRESHOLD,
  pressureColor,
} from "../types";
export * from "./editors";
export * from "./issues";
export type { NetworkSummary } from "./NetworkDataContext";
export {
  NetworkDataProvider,
  useNetworkData,
} from "./NetworkDataContext";
// Re-export so callers only need to import from the data seam.
export { useNetworkVersion } from "./NetworkVersionContext";
export * from "./network";
export * from "./projects";
export * from "./queue";
export * from "./results";
export * from "./scenarios";
export * from "./simulation";
