/** The block-driven analysis surface: fetch every catalog block for the
 * active target and render its fragment — the analysis-as-blocks
 * convergence, shared by every engine.
 *
 * Engine-specific concerns arrive as props, never as switches: a `header`
 * (the wds criteria editor), and `criteria` forwarded to the backend,
 * which maps them onto the criteria-shaped blocks' options with the
 * engine's own unit factors. Criteria travel with the request rather than
 * being re-read from disk so an edit can never race its own save.
 */

import { useEffect, useState } from "react";
import { useAppState, useSimulation } from "../../AppContext";
import { tryInvokeOr } from "../../hooks/ipc";
import { useUnitSystem } from "../../units";
import { BlockPanel } from "./FragmentView";
import { type AnalysisBlock, blockSpan } from "./fragments";

/** How long an edit may keep changing before the blocks refetch. Criteria
 * sliders emit per-tick; re-producing every block per tick would contend
 * with the drag itself. */
const REFETCH_DEBOUNCE_MS = 300;

export function BlockAnalysisView({
  header,
  criteria,
}: {
  /** Rendered above the panels; scrolls with them. */
  header?: React.ReactNode;
  /** Forwarded verbatim to the backend's criteria→options mapping. The
   * object's JSON identity is the refetch key. */
  criteria?: unknown;
}) {
  const { activeProjectId, activeScenarioId } = useAppState();
  const { resultGeneration } = useSimulation();
  // Tagged block values arrive already re-expressed in the reader's
  // resolved system (report spec §4.0), so this view renders what it is
  // given, and flipping the preference refetches.
  const unitSystem = useUnitSystem();
  const [blocks, setBlocks] = useState<AnalysisBlock[] | null>(null);

  // Serialised so the effect re-runs on value changes, not on the fresh
  // object identity every render produces.
  const criteriaKey = criteria === undefined ? null : JSON.stringify(criteria);

  // resultGeneration is a re-run token: a completed run bumps it so the
  // panels refresh with the new results.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-run token, see above
  useEffect(() => {
    if (!activeProjectId) return;
    let cancelled = false;
    const handle = window.setTimeout(
      () => {
        void tryInvokeOr<AnalysisBlock[]>(
          "get_analysis_blocks",
          {
            projectId: activeProjectId,
            scenarioId: activeScenarioId,
            unitSystem,
            criteria: criteriaKey === null ? null : JSON.parse(criteriaKey),
          },
          [],
        ).then((b) => {
          if (!cancelled) setBlocks(b);
        });
      },
      // Only edits debounce; the first load and target switches fetch at
      // once, because there the wait would just be a blank page.
      blocks === null ? 0 : REFETCH_DEBOUNCE_MS,
    );
    return () => {
      cancelled = true;
      window.clearTimeout(handle);
    };
  }, [
    activeProjectId,
    activeScenarioId,
    resultGeneration,
    unitSystem,
    criteriaKey,
  ]);

  // A target switch shows loading rather than the previous target's panels.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the deps ARE the point — this effect exists to fire on target switches, and reads nothing else.
  useEffect(() => {
    setBlocks(null);
  }, [activeProjectId, activeScenarioId]);

  if (!activeProjectId) return null;
  if (blocks === null) {
    return (
      <div style={{ padding: 18 }}>
        {header}
        <div style={{ paddingTop: 12, color: "var(--text-tertiary)" }}>
          Loading…
        </div>
      </div>
    );
  }
  return (
    <div
      style={{
        flex: 1,
        overflowY: "auto",
        padding: 18,
        // Two-ish columns on a wide window, one on a narrow one; each
        // block claims a cell or the whole row by its fragment's shape —
        // presentation decided here, from neutral data, never by hints
        // from an engine (hydra-common §3).
        display: "grid",
        gridTemplateColumns: "repeat(auto-fill, minmax(min(520px, 100%), 1fr))",
        gap: 12,
        alignItems: "start",
      }}
    >
      {header ? <div style={{ gridColumn: "1 / -1" }}>{header}</div> : null}
      {blocks.length === 0 ? (
        <div
          style={{ color: "var(--text-tertiary)", fontSize: "var(--text-md)" }}
        >
          Run a simulation to see results here.
        </div>
      ) : (
        blocks.map((b) => (
          <div
            key={b.id}
            style={
              blockSpan(b.fragment) === "full"
                ? { gridColumn: "1 / -1" }
                : undefined
            }
          >
            <BlockPanel block={b} />
          </div>
        ))
      )}
    </div>
  );
}
