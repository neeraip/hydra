/** The block-driven analysis surface: fetch every catalog block for the
 * active target and render its fragment — the analysis-as-blocks
 * convergence, shared by every engine.
 *
 * Blocks arrive grouped by their engine-authored category (hydra-common
 * §3.2), and the categories present become the tab bar — derived from the
 * data, so an engine reshapes this page by editing only its catalog. One
 * category means no tabs: a small engine is not forced to wear chrome.
 *
 * Engine-specific concerns arrive as props, never as switches: `criteria`
 * is forwarded to the backend, which maps it onto the criteria-shaped
 * blocks' options with the engine's own unit factors. It travels with the
 * request rather than being re-read from disk so an edit — made from the
 * project toolbar, which owns the criteria control — can never race its
 * own save.
 */

import { useEffect, useState } from "react";
import { useAppState, useSimulation } from "../../AppContext";
import { fetchInto } from "../../hooks/fetchInto";
import { tryInvokeOr } from "../../hooks/ipc";
import { useUnitSystem } from "../../units";
import { BlockPanel } from "./FragmentView";
import {
  type AnalysisBlock,
  activeCategoryOf,
  categoriesOf,
  layoutSpans,
} from "./fragments";

/** How long an edit may keep changing before the blocks refetch. Criteria
 * sliders emit per-tick; re-producing every block per tick would contend
 * with the drag itself. */
const REFETCH_DEBOUNCE_MS = 300;

export function BlockAnalysisView({
  criteria,
}: {
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
  const [pickedCategory, setPickedCategory] = useState<string | null>(null);

  // Serialised so the effect re-runs on value changes, not on the fresh
  // object identity every render produces.
  const criteriaKey = criteria === undefined ? null : JSON.stringify(criteria);

  // resultGeneration is a re-run token: a completed run bumps it so the
  // panels refresh with the new results.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-run token, see above
  useEffect(() => {
    if (!activeProjectId) return;
    let cancel = () => {};
    const handle = window.setTimeout(
      () => {
        cancel = fetchInto(
          tryInvokeOr<AnalysisBlock[]>(
            "get_analysis_blocks",
            {
              projectId: activeProjectId,
              scenarioId: activeScenarioId,
              unitSystem,
              criteria: criteriaKey === null ? null : JSON.parse(criteriaKey),
            },
            [],
          ),
          setBlocks,
        );
      },
      // Only edits debounce; the first load and target switches fetch at
      // once, because there the wait would just be a blank page.
      blocks === null ? 0 : REFETCH_DEBOUNCE_MS,
    );
    return () => {
      window.clearTimeout(handle);
      cancel();
    };
  }, [
    activeProjectId,
    activeScenarioId,
    resultGeneration,
    unitSystem,
    criteriaKey,
  ]);

  // A target switch shows loading rather than the previous target's
  // panels, on that target's tab.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the deps ARE the point — this effect exists to fire on target switches, and reads nothing else.
  useEffect(() => {
    setBlocks(null);
    setPickedCategory(null);
  }, [activeProjectId, activeScenarioId]);

  if (!activeProjectId) return null;

  const categories = blocks === null ? [] : categoriesOf(blocks);
  const activeCategory = activeCategoryOf(pickedCategory, categories);
  const visible =
    activeCategory === null
      ? (blocks ?? [])
      : (blocks ?? []).filter((b) => b.category === activeCategory);
  const spans = layoutSpans(visible.map((b) => b.fragment));

  return (
    <div
      style={{
        flex: 1,
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        minHeight: 0,
      }}
    >
      {categories.length > 1 && (
        <div
          style={{
            flexShrink: 0,
            display: "flex",
            alignItems: "flex-end",
            gap: 2,
            padding: "0 18px",
            borderBottom: "1px solid var(--border)",
            background: "var(--bg-panel)",
          }}
        >
          {categories.map((category) => {
            const isActive = category === activeCategory;
            return (
              <button
                type="button"
                key={category}
                onClick={() => setPickedCategory(category)}
                style={{
                  padding: "10px 16px 9px",
                  fontSize: "var(--text-md)",
                  fontWeight: isActive ? 600 : 500,
                  fontFamily: "var(--font-ui)",
                  color: isActive
                    ? "var(--text-primary)"
                    : "var(--text-tertiary)",
                  background: "transparent",
                  border: "none",
                  borderBottom: isActive
                    ? "2px solid var(--accent)"
                    : "2px solid transparent",
                  cursor: "pointer",
                  transition: "color var(--t-fast), border-color var(--t-fast)",
                  whiteSpace: "nowrap",
                  marginBottom: -1,
                }}
                onMouseEnter={(e) => {
                  if (!isActive)
                    (e.currentTarget as HTMLButtonElement).style.color =
                      "var(--text-secondary)";
                }}
                onMouseLeave={(e) => {
                  if (!isActive)
                    (e.currentTarget as HTMLButtonElement).style.color =
                      "var(--text-tertiary)";
                }}
              >
                {category}
              </button>
            );
          })}
        </div>
      )}
      {blocks === null ? (
        <div style={{ padding: 18, color: "var(--text-tertiary)" }}>
          Loading…
        </div>
      ) : (
        <div className="analysis-scroll">
          {/* One or two columns (app.css, container query); each block
              claims a cell or the whole row by its fragment's shape,
              paired so no row is left ragged (`layoutSpans`) —
              presentation decided here, from neutral data, never by
              hints from an engine (hydra-common §3). */}
          <div className="analysis-grid">
            {blocks.length === 0 ? (
              <div
                style={{
                  color: "var(--text-tertiary)",
                  fontSize: "var(--text-md)",
                }}
              >
                Run a simulation to see results here.
              </div>
            ) : (
              visible.map((b, i) => (
                <div
                  key={b.id}
                  style={
                    spans[i] === "full" ? { gridColumn: "1 / -1" } : undefined
                  }
                >
                  <BlockPanel block={b} />
                </div>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  );
}
