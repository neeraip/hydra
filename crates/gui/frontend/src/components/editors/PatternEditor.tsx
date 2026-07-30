/* Time-pattern editor — 24-hour multiplier bars with editable values.
   Edits are staged into the shared DraftContext, not committed to the
   backend immediately — they become part of the unified Network Editor
   draft alongside Elements/Curves/Controls, saved or discarded together. */

import { TrashIcon } from "@heroicons/react/16/solid";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import { useAppState } from "../../AppContext";
import { renamePattern, type TimePattern, usePatterns } from "../../hooks";
import { useDraft } from "../../hooks/DraftContext";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";
import { inpIdError } from "../../inpId";
import { DeleteConfirmModal } from "../modals/DeleteConfirmModal";
import { EditorSidebarList } from "./EditorSidebarList";
import {
  downsampleMinMax,
  envelopePath,
  resizePattern,
} from "./patternDownsample";

const DEFAULT_PATTERN_MULTIPLIERS: number[] = new Array(24).fill(1.0);

/** Above this many multipliers the interactive per-bar strip (one DOM
 * element per step) switches to a downsampled SVG envelope — an
 * hourly-for-a-year pattern has 8,760 steps and per-bar DOM at that size
 * is the Issues-panel freeze class. Editing stays available through the
 * numeric grid, which virtualizes at the same scale. */
const MAX_INTERACTIVE_BARS = 168;

/** SVG envelope resolution (buckets) for long patterns. */
const ENVELOPE_BUCKETS = 200;

/** Numeric-grid inputs per row (mirrors the 12-column grid layout). */
const GRID_COLS = 12;

/** Above this many multipliers the numeric grid virtualizes its rows inside
 * a fixed-height scroller instead of mounting one input per step. */
const MAX_UNVIRTUALIZED_GRID_VALUES = 192;

export function PatternEditor({ accent }: { accent: string }) {
  const { showToast } = useAppState();
  const { bumpNetwork } = useNetworkVersion();
  const rawPatterns = usePatterns();
  const {
    patternAdds,
    setPatternAdds,
    patternEdits,
    setPatternEdits,
    patternDeletes,
    setPatternDeletes,
  } = useDraft();

  // Merge staged creates/edits/deletes on top of the real pattern list so
  // the sidebar and editor always reflect the current draft.
  const patterns = useMemo<TimePattern[]>(() => {
    const existing = rawPatterns
      .filter((p) => !patternDeletes.has(p.id))
      .map((p) => ({
        id: p.id,
        label: p.id,
        multipliers: patternEdits.get(p.id) ?? p.multipliers,
        stepHours: 1,
      }));
    const added: TimePattern[] = Array.from(patternAdds.entries()).map(
      ([id, multipliers]) => ({ id, label: id, multipliers, stepHours: 1 }),
    );
    return [...existing, ...added];
  }, [rawPatterns, patternEdits, patternDeletes, patternAdds]);

  const [activeId, setActiveId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [newId, setNewId] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const [pendingDeleteId, setPendingDeleteId] = useState<string | null>(null);
  const newIdRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (creating) newIdRef.current?.focus();
  }, [creating]);

  const effectiveId = activeId ?? patterns[0]?.id ?? "";
  const pattern = patterns.find((p) => p.id === effectiveId) ?? null;
  const multipliers = pattern?.multipliers ?? [];
  const isNew = patternAdds.has(effectiveId);
  const isOverridden = isNew || patternEdits.has(effectiveId);

  function stageMultipliers(next: number[]) {
    if (isNew) {
      setPatternAdds((prev) => new Map(prev).set(effectiveId, next));
    } else {
      setPatternEdits((prev) => new Map(prev).set(effectiveId, next));
    }
  }

  function handleCreate() {
    const trimmed = newId.trim();
    // INP cannot represent a space in an id (see inpIdError): accepting one
    // writes a [PATTERNS] line that reads back as a bad multiplier.
    const badFormat = inpIdError(newId);
    if (badFormat) {
      setCreateError(badFormat);
      return;
    }
    if (rawPatterns.some((p) => p.id === trimmed) || patternAdds.has(trimmed)) {
      setCreateError(`pattern '${trimmed}' already exists`);
      return;
    }
    setPatternAdds((prev) =>
      new Map(prev).set(trimmed, DEFAULT_PATTERN_MULTIPLIERS),
    );
    setActiveId(trimmed);
    setCreating(false);
    setNewId("");
    setCreateError(null);
  }

  function handleDelete() {
    if (!pendingDeleteId) return;
    const id = pendingDeleteId;
    setPendingDeleteId(null);
    if (patternAdds.has(id)) {
      setPatternAdds((prev) => {
        const next = new Map(prev);
        next.delete(id);
        return next;
      });
    } else {
      setPatternDeletes((prev) => new Set(prev).add(id));
      setPatternEdits((prev) => {
        if (!prev.has(id)) return prev;
        const next = new Map(prev);
        next.delete(id);
        return next;
      });
    }
    if (activeId === id) setActiveId(null);
  }

  // Renaming affects the ID used as the key throughout the draft's
  // patternAdds/patternEdits maps, so (unlike other pattern edits) it isn't
  // staged — it's applied immediately, same as most single-shot renames
  // elsewhere in the app (project/scenario rename).
  async function handleRename(oldId: string, rawNewId: string) {
    const trimmed = rawNewId.trim();
    if (!trimmed || trimmed === oldId) return;
    const badFormat = inpIdError(trimmed);
    if (badFormat) {
      showToast(badFormat, "error");
      return;
    }
    if (
      rawPatterns.some((p) => p.id === trimmed) ||
      (patternAdds.has(trimmed) && trimmed !== oldId)
    ) {
      showToast(`pattern '${trimmed}' already exists`, "error");
      return;
    }
    if (patternAdds.has(oldId)) {
      // Not yet created — just re-key the local draft entry.
      setPatternAdds((prev) => {
        const next = new Map(prev);
        const multipliers = next.get(oldId);
        next.delete(oldId);
        if (multipliers) next.set(trimmed, multipliers);
        return next;
      });
      setActiveId(trimmed);
      return;
    }
    try {
      await renamePattern(oldId, trimmed);
      bumpNetwork();
      setPatternEdits((prev) => {
        if (!prev.has(oldId)) return prev;
        const next = new Map(prev);
        const m = next.get(oldId);
        next.delete(oldId);
        if (m) next.set(trimmed, m);
        return next;
      });
      setActiveId(trimmed);
    } catch (err) {
      showToast(
        typeof err === "string" ? err : "Failed to rename pattern",
        "error",
      );
    }
  }

  return (
    <div style={{ flex: 1, display: "flex", overflow: "hidden", minHeight: 0 }}>
      {/* Pattern list (virtualized — networks can carry thousands of patterns) */}
      <EditorSidebarList
        items={patterns}
        getKey={(p) => p.id}
        renderItem={(p) => {
          const active = p.id === effectiveId;
          const isDirty =
            patternAdds.has(p.id) ||
            patternEdits.has(p.id) ||
            patternDeletes.has(p.id);
          return (
            <button
              type="button"
              onClick={() => setActiveId(p.id)}
              style={{
                display: "block",
                width: "100%",
                textAlign: "left",
                padding: "10px 12px",
                border: "none",
                background: active ? `${accent}1f` : "transparent",
                borderLeft: active
                  ? `2px solid ${accent}`
                  : "2px solid transparent",
                cursor: "pointer",
                fontFamily: "var(--font-ui)",
                color: active ? "var(--text-primary)" : "var(--text-secondary)",
                borderBottom: "1px solid var(--border)",
                opacity: patternDeletes.has(p.id) ? 0.5 : 1,
              }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  fontSize: "var(--text-lg)",
                  fontWeight: 500,
                  fontFamily: "var(--font-mono)",
                }}
              >
                {p.id}
                {isDirty && (
                  <span
                    style={{
                      width: 6,
                      height: 6,
                      borderRadius: "50%",
                      background: "rgba(220, 160, 40, 0.9)",
                      display: "inline-block",
                      flexShrink: 0,
                    }}
                  />
                )}
              </div>
              <div
                style={{
                  fontSize: "var(--text-sm)",
                  color: "var(--text-tertiary)",
                  marginTop: 2,
                }}
              >
                {p.label}
              </div>
            </button>
          );
        }}
        footer={
          creating ? (
            <div
              style={{
                padding: "8px 12px",
                borderBottom: "1px solid var(--border)",
              }}
            >
              <input
                ref={newIdRef}
                value={newId}
                onChange={(e) => {
                  setNewId(e.target.value);
                  setCreateError(null);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleCreate();
                  if (e.key === "Escape") {
                    setCreating(false);
                    setNewId("");
                    setCreateError(null);
                  }
                }}
                placeholder="Pattern ID…"
                style={{
                  width: "100%",
                  height: 26,
                  background: "var(--bg-input)",
                  border: `1px solid ${createError ? "var(--status-error)" : "var(--border-focus)"}`,
                  borderRadius: 4,
                  padding: "0 6px",
                  color: "var(--text-primary)",
                  fontFamily: "var(--font-mono)",
                  fontSize: "var(--text-md)",
                  outline: "none",
                  boxSizing: "border-box",
                }}
              />
              {createError && (
                <div
                  style={{
                    fontSize: "var(--text-sm)",
                    color: "var(--status-error)",
                    marginTop: 3,
                  }}
                >
                  {createError}
                </div>
              )}
              <div style={{ display: "flex", gap: 4, marginTop: 6 }}>
                <button
                  type="button"
                  onClick={handleCreate}
                  style={{
                    flex: 1,
                    height: 24,
                    fontSize: "var(--text-sm)",
                    background: "var(--accent)",
                    color: "#fff",
                    border: "none",
                    borderRadius: 4,
                    cursor: "pointer",
                  }}
                >
                  Add
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setCreating(false);
                    setNewId("");
                    setCreateError(null);
                  }}
                  style={{
                    flex: 1,
                    height: 24,
                    fontSize: "var(--text-sm)",
                    background: "var(--nav-hover)",
                    color: "var(--text-secondary)",
                    border: "none",
                    borderRadius: 4,
                    cursor: "pointer",
                  }}
                >
                  Cancel
                </button>
              </div>
            </div>
          ) : (
            <button
              type="button"
              onClick={() => setCreating(true)}
              style={{
                width: "100%",
                padding: "10px 12px",
                border: "none",
                background: "transparent",
                color: "var(--text-tertiary)",
                cursor: "pointer",
                fontSize: "var(--text-md)",
                fontFamily: "var(--font-ui)",
                textAlign: "left",
              }}
            >
              + New pattern
            </button>
          )
        }
      />

      {/* Right pane */}
      {pattern ? (
        <div
          style={{
            flex: 1,
            display: "flex",
            flexDirection: "column",
            overflow: "hidden",
            minHeight: 0,
          }}
        >
          <PatternHeader
            pattern={pattern}
            accent={accent}
            multipliers={multipliers}
            onDelete={() => setPendingDeleteId(pattern.id)}
            onRename={(newId) => handleRename(pattern.id, newId)}
          />

          <div
            style={{
              flex: 1,
              padding: 16,
              display: "flex",
              flexDirection: "column",
              gap: 12,
              overflow: "auto",
            }}
          >
            <PatternBars
              multipliers={multipliers}
              accent={accent}
              stepHours={pattern.stepHours}
              onChange={(idx, val) => {
                const next = [...multipliers];
                next[idx] = val;
                stageMultipliers(next);
              }}
            />
            <PatternStepControls
              count={multipliers.length}
              onAdd={() =>
                stageMultipliers([
                  ...multipliers,
                  multipliers[multipliers.length - 1] ?? 1,
                ])
              }
              onRemoveLast={() =>
                multipliers.length > 1 &&
                stageMultipliers(multipliers.slice(0, -1))
              }
              onResize={(n) => stageMultipliers(resizePattern(multipliers, n))}
            />
            <PatternRow
              multipliers={multipliers}
              stepHours={pattern.stepHours}
              accent={accent}
              onChange={(idx, val) => {
                const next = [...multipliers];
                next[idx] = val;
                stageMultipliers(next);
              }}
              onReset={() => {
                if (isNew) {
                  setPatternAdds((prev) =>
                    new Map(prev).set(effectiveId, DEFAULT_PATTERN_MULTIPLIERS),
                  );
                } else {
                  setPatternEdits((prev) => {
                    if (!prev.has(effectiveId)) return prev;
                    const next = new Map(prev);
                    next.delete(effectiveId);
                    return next;
                  });
                }
              }}
              isOverridden={isOverridden}
            />
          </div>
        </div>
      ) : (
        <div
          style={{
            flex: 1,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "var(--text-tertiary)",
            fontSize: "var(--text-lg)",
          }}
        >
          No time patterns defined. Use "+ New pattern" to create one.
        </div>
      )}
      <DeleteConfirmModal
        open={pendingDeleteId != null}
        elementKind="pattern"
        elementId={pendingDeleteId ?? ""}
        onConfirm={handleDelete}
        onCancel={() => setPendingDeleteId(null)}
      />
    </div>
  );
}

function PatternHeader({
  pattern,
  accent,
  multipliers,
  onDelete,
  onRename,
}: {
  pattern: TimePattern;
  accent: string;
  multipliers: number[];
  onDelete: () => void;
  onRename: (newId: string) => void;
}) {
  const min = Math.min(...multipliers);
  const max = Math.max(...multipliers);
  const mean = multipliers.reduce((a, b) => a + b, 0) / multipliers.length;
  const [nameDraft, setNameDraft] = useState(pattern.id);
  const prevPatternId = useRef(pattern.id);
  if (pattern.id !== prevPatternId.current) {
    prevPatternId.current = pattern.id;
    setNameDraft(pattern.id);
  }
  return (
    <div
      style={{
        padding: "12px 16px",
        borderBottom: "1px solid var(--border)",
        display: "flex",
        alignItems: "center",
        gap: 16,
      }}
    >
      <input
        value={nameDraft}
        onChange={(e) => setNameDraft(e.target.value)}
        onBlur={() => {
          if (nameDraft.trim() !== pattern.id) onRename(nameDraft);
          else setNameDraft(pattern.id);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter") (e.target as HTMLInputElement).blur();
          if (e.key === "Escape") {
            setNameDraft(pattern.id);
            (e.target as HTMLInputElement).blur();
          }
        }}
        style={{
          fontSize: "var(--text-2xl)",
          fontWeight: 600,
          color: "var(--text-primary)",
          fontFamily: "var(--font-mono)",
          background: "transparent",
          border: "1px solid transparent",
          borderRadius: 4,
          padding: "2px 6px",
          outline: "none",
          width: 140,
        }}
        onFocus={(e) => {
          e.currentTarget.style.border = "1px solid var(--border-focus)";
          e.currentTarget.style.background = "var(--bg-input, var(--bg-app))";
        }}
      />
      <div
        style={{
          marginLeft: "auto",
          display: "flex",
          gap: 16,
          fontSize: "var(--text-sm)",
          color: "var(--text-tertiary)",
        }}
      >
        <Stat label="Step" value={`${pattern.stepHours}h`} />
        <Stat label="Length" value={`${multipliers.length}`} />
        <Stat label="Min" value={min.toFixed(2)} accent={accent} />
        <Stat label="Mean" value={mean.toFixed(2)} accent={accent} />
        <Stat label="Max" value={max.toFixed(2)} accent={accent} />
      </div>
      <button
        type="button"
        onClick={onDelete}
        title="Delete pattern"
        style={{
          flexShrink: 0,
          border: "none",
          background: "transparent",
          color: "var(--text-tertiary)",
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          padding: 4,
        }}
        onMouseEnter={(e) => {
          (e.currentTarget as HTMLButtonElement).style.color = "#ef4444";
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLButtonElement).style.color =
            "var(--text-tertiary)";
        }}
      >
        <TrashIcon style={{ width: 14, height: 14 }} />
      </button>
    </div>
  );
}
function Stat({
  label,
  value,
  accent,
}: {
  label: string;
  value: string;
  accent?: string;
}) {
  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        alignItems: "flex-end",
      }}
    >
      <span
        style={{
          fontSize: "var(--text-2xs)",
          textTransform: "uppercase",
          letterSpacing: 0.4,
        }}
      >
        {label}
      </span>
      <span
        style={{
          fontSize: "var(--text-lg)",
          fontFamily: "var(--font-mono)",
          color: accent ?? "var(--text-primary)",
        }}
      >
        {value}
      </span>
    </div>
  );
}

function PatternBars({
  multipliers,
  accent,
  stepHours,
  onChange,
}: {
  multipliers: number[];
  accent: string;
  stepHours: number;
  onChange: (idx: number, val: number) => void;
}) {
  const H = 220;
  const yMax = Math.max(2.0, Math.max(...multipliers) * 1.05);
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [dragIdx, setDragIdx] = useState<number | null>(null);
  const [hoverIdx, setHoverIdx] = useState<number | null>(null);

  // Long patterns (e.g. 8,760 hourly steps for a year): per-bar DOM would
  // freeze the tab, so render a downsampled SVG envelope instead. Values
  // remain editable through the numeric grid below.
  if (multipliers.length > MAX_INTERACTIVE_BARS) {
    return (
      <PatternEnvelope
        multipliers={multipliers}
        accent={accent}
        stepHours={stepHours}
        height={H}
        yMax={yMax}
      />
    );
  }

  function handleMove(e: React.MouseEvent, idx: number) {
    if (dragIdx !== idx || !containerRef.current) return;
    const cell = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const rel = 1 - (e.clientY - cell.top) / cell.height;
    const val = Math.max(0, Math.min(yMax, rel * yMax));
    onChange(idx, parseFloat(val.toFixed(2)));
  }

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: drag surface is intentionally pointer-driven.
    <div
      ref={containerRef}
      onMouseUp={() => setDragIdx(null)}
      onMouseLeave={() => setDragIdx(null)}
      style={{
        display: "grid",
        gridTemplateColumns: `repeat(${multipliers.length}, 1fr)`,
        height: H,
        gap: 2,
        background: "var(--bg-app)",
        border: "1px solid var(--border)",
        borderRadius: 4,
        padding: "8px 8px 22px",
        position: "relative",
      }}
    >
      {/* Reference line at 1.0 */}
      <div
        style={{
          position: "absolute",
          left: 8,
          right: 8,
          top: 8 + (1 - 1.0 / yMax) * (H - 30),
          borderTop: "1px dashed var(--border-hover)",
          pointerEvents: "none",
        }}
      />
      {multipliers.map((m, i) => {
        const ratio = m / yMax;
        const isActive = dragIdx === i || hoverIdx === i;
        const hour = i * stepHours;
        return (
          // biome-ignore lint/a11y/noStaticElementInteractions: bars support pointer dragging only.
          <div
            key={`${hour}`}
            onMouseDown={() => setDragIdx(i)}
            onMouseEnter={() => setHoverIdx(i)}
            onMouseLeave={() => setHoverIdx(null)}
            onMouseMove={(e) => handleMove(e, i)}
            style={{
              position: "relative",
              display: "flex",
              alignItems: "flex-end",
              cursor: "ns-resize",
              userSelect: "none",
            }}
          >
            <div
              style={{
                width: "100%",
                height: `${ratio * 100}%`,
                background: isActive ? accent : `${accent}99`,
                borderRadius: "2px 2px 0 0",
                transition: dragIdx === i ? "none" : "background 80ms",
                boxShadow: isActive ? `0 0 6px ${accent}66` : undefined,
              }}
            />
            {isActive && (
              <div
                style={{
                  position: "absolute",
                  top: -18,
                  left: "50%",
                  transform: "translateX(-50%)",
                  fontSize: "var(--text-xs)",
                  fontFamily: "var(--font-mono)",
                  color: accent,
                  background: "var(--bg-overlay)",
                  padding: "1px 4px",
                  borderRadius: 2,
                  whiteSpace: "nowrap",
                }}
              >
                {m.toFixed(2)}
              </div>
            )}
            {i % Math.max(1, Math.floor(multipliers.length / 8)) === 0 && (
              <div
                style={{
                  position: "absolute",
                  bottom: -16,
                  left: "50%",
                  transform: "translateX(-50%)",
                  fontSize: "var(--text-2xs)",
                  color: "var(--text-tertiary)",
                  fontFamily: "var(--font-mono)",
                }}
              >
                {(i * stepHours).toString().padStart(2, "0")}h
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

/** Downsampled min/max envelope for patterns too long to edit per-bar.
 * Fixed DOM cost (a handful of SVG elements) regardless of pattern length. */
function PatternEnvelope({
  multipliers,
  accent,
  stepHours,
  height,
  yMax,
}: {
  multipliers: number[];
  accent: string;
  stepHours: number;
  height: number;
  yMax: number;
}) {
  const VW = 1000;
  const innerH = height - 30;
  const buckets = useMemo(
    () => downsampleMinMax(multipliers, ENVELOPE_BUCKETS),
    [multipliers],
  );
  const band = useMemo(
    () => envelopePath(buckets, VW, innerH, yMax),
    [buckets, innerH, yMax],
  );
  const refY = innerH - (1.0 / yMax) * innerH;
  const totalHours = multipliers.length * stepHours;
  return (
    <div
      style={{
        height,
        background: "var(--bg-app)",
        border: "1px solid var(--border)",
        borderRadius: 4,
        padding: "8px 8px 22px",
        position: "relative",
        boxSizing: "border-box",
      }}
    >
      <svg
        width="100%"
        height={innerH}
        viewBox={`0 0 ${VW} ${innerH}`}
        preserveAspectRatio="none"
        style={{ display: "block" }}
      >
        <title>Pattern preview (downsampled)</title>
        <path d={band} fill={`${accent}66`} stroke={accent} strokeWidth={1} />
        <line
          x1={0}
          x2={VW}
          y1={refY}
          y2={refY}
          stroke="var(--border-hover)"
          strokeDasharray="4 6"
        />
      </svg>
      <div
        style={{
          position: "absolute",
          bottom: 4,
          left: 8,
          right: 8,
          display: "flex",
          justifyContent: "space-between",
          fontSize: "var(--text-2xs)",
          color: "var(--text-tertiary)",
          fontFamily: "var(--font-mono)",
        }}
      >
        <span>00h</span>
        <span style={{ fontFamily: "var(--font-ui)", fontStyle: "italic" }}>
          {multipliers.length} steps — downsampled preview; edit values in the
          numeric grid below
        </span>
        <span>{totalHours}h</span>
      </div>
    </div>
  );
}

function PatternRow({
  multipliers,
  stepHours,
  accent,
  onChange,
  onReset,
  isOverridden,
}: {
  multipliers: number[];
  stepHours: number;
  accent: string;
  onChange: (idx: number, val: number) => void;
  onReset: () => void;
  isOverridden: boolean;
}) {
  const hours = multipliers.map((_multiplier, i) => i * stepHours);
  return (
    <div>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          marginBottom: 6,
          fontSize: "var(--text-sm)",
          fontWeight: 500,
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
          letterSpacing: 0.4,
        }}
      >
        Numeric values
        {isOverridden && (
          <span
            style={{
              marginLeft: 8,
              color: accent,
              textTransform: "none",
              letterSpacing: 0,
            }}
          >
            · edited
          </span>
        )}
        <button
          type="button"
          onClick={onReset}
          disabled={!isOverridden}
          style={{
            marginLeft: "auto",
            border: "1px solid var(--border)",
            background: "transparent",
            color: isOverridden
              ? "var(--text-secondary)"
              : "var(--text-disabled)",
            cursor: isOverridden ? "pointer" : "not-allowed",
            padding: "3px 8px",
            borderRadius: 4,
            fontSize: "var(--text-sm)",
            fontFamily: "var(--font-ui)",
            textTransform: "none",
            letterSpacing: 0,
          }}
        >
          Reset
        </button>
      </div>
      {multipliers.length <= MAX_UNVIRTUALIZED_GRID_VALUES ? (
        <div
          style={{
            display: "grid",
            gridTemplateColumns: `repeat(${Math.min(GRID_COLS, multipliers.length)}, minmax(60px, 1fr))`,
            gap: 6,
          }}
        >
          {multipliers.map((m, i) => (
            <GridCell
              key={`${hours[i]}`}
              hour={hours[i]}
              value={m}
              index={i}
              onChange={onChange}
            />
          ))}
        </div>
      ) : (
        <VirtualizedGrid
          multipliers={multipliers}
          hours={hours}
          onChange={onChange}
        />
      )}
    </div>
  );
}

/** One labelled number input of the numeric grid. */
function GridCell({
  hour,
  value,
  index,
  onChange,
}: {
  hour: number;
  value: number;
  index: number;
  onChange: (idx: number, val: number) => void;
}) {
  return (
    <label style={{ display: "flex", flexDirection: "column", gap: 2 }}>
      <span
        style={{
          fontSize: "var(--text-2xs)",
          color: "var(--text-tertiary)",
          fontFamily: "var(--font-mono)",
        }}
      >
        {hour.toString().padStart(2, "0")}:00
      </span>
      <input
        type="number"
        step="0.05"
        value={value}
        onChange={(e) => {
          const v = parseFloat(e.target.value);
          if (!Number.isNaN(v)) onChange(index, v);
        }}
        style={{
          width: "100%",
          height: 26,
          background: "var(--bg-input, var(--bg-card))",
          border: "1px solid var(--border)",
          borderRadius: 4,
          color: "var(--text-primary)",
          fontSize: "var(--text-md)",
          fontFamily: "var(--font-mono)",
          padding: "0 6px",
          outline: "none",
        }}
      />
    </label>
  );
}

/** Estimated height of one virtualized grid row: 9px label + 2px gap +
 * 26px input + 6px row gap. */
const GRID_ROW_ESTIMATE = 50;

/** Windowed variant of the numeric grid for very long patterns: rows of
 * {@link GRID_COLS} inputs are virtualized inside a fixed-height scroller so
 * an 8,760-step pattern mounts ~dozens of inputs instead of thousands.
 * Per-cell editing semantics are identical to the plain grid. */
function VirtualizedGrid({
  multipliers,
  hours,
  onChange,
}: {
  multipliers: number[];
  hours: number[];
  onChange: (idx: number, val: number) => void;
}) {
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const rowCount = Math.ceil(multipliers.length / GRID_COLS);
  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => GRID_ROW_ESTIMATE,
    overscan: 6,
  });
  return (
    <div
      ref={scrollRef}
      style={{
        height: 320,
        overflowY: "auto",
        border: "1px solid var(--border)",
        borderRadius: 4,
        padding: "6px 8px",
      }}
    >
      <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
        {virtualizer.getVirtualItems().map((vi) => {
          const start = vi.index * GRID_COLS;
          const end = Math.min(multipliers.length, start + GRID_COLS);
          const cells = [];
          for (let i = start; i < end; i++) {
            cells.push(
              <GridCell
                key={`${hours[i]}`}
                hour={hours[i]}
                value={multipliers[i]}
                index={i}
                onChange={onChange}
              />,
            );
          }
          return (
            <div
              key={vi.key}
              ref={virtualizer.measureElement}
              data-index={vi.index}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                width: "100%",
                transform: `translateY(${vi.start}px)`,
                display: "grid",
                gridTemplateColumns: `repeat(${GRID_COLS}, minmax(60px, 1fr))`,
                gap: 6,
                paddingBottom: 6,
              }}
            >
              {cells}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Step-count controls: append (copies the last value), remove-last, and a
 * resize field. Growing cycle-repeats the existing sequence so a daily
 * pattern tiles cleanly into longer horizons. */
function PatternStepControls({
  count,
  onAdd,
  onRemoveLast,
  onResize,
}: {
  count: number;
  onAdd: () => void;
  onRemoveLast: () => void;
  onResize: (n: number) => void;
}) {
  const [resizeDraft, setResizeDraft] = useState("");
  const btnStyle: React.CSSProperties = {
    background: "var(--bg-card)",
    border: "1px solid var(--border)",
    borderRadius: 4,
    color: "var(--text-secondary)",
    fontSize: "var(--text-sm)",
    padding: "3px 8px",
    cursor: "pointer",
  };
  function applyResize() {
    const n = Number(resizeDraft.trim());
    if (!Number.isFinite(n) || n < 1) return;
    onResize(Math.min(8784, Math.floor(n)));
    setResizeDraft("");
  }
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        fontSize: "var(--text-sm)",
        color: "var(--text-tertiary)",
      }}
    >
      <span>
        {count} step{count === 1 ? "" : "s"}
      </span>
      <button
        type="button"
        style={btnStyle}
        onClick={onAdd}
        data-tooltip="Append a step (copies the last multiplier)"
      >
        + Add step
      </button>
      <button
        type="button"
        style={{
          ...btnStyle,
          opacity: count <= 1 ? 0.4 : 1,
          cursor: count <= 1 ? "default" : "pointer",
        }}
        onClick={onRemoveLast}
        disabled={count <= 1}
        data-tooltip="Remove the last step"
      >
        − Remove last
      </button>
      <span style={{ marginLeft: 8 }}>Resize to</span>
      <input
        value={resizeDraft}
        onChange={(e) => setResizeDraft(e.target.value.replace(/[^0-9]/g, ""))}
        onKeyDown={(e) => e.key === "Enter" && applyResize()}
        placeholder={String(count)}
        style={{
          width: 56,
          background: "var(--bg-input, rgba(255,255,255,0.05))",
          border: "1px solid var(--border)",
          borderRadius: 4,
          color: "var(--text-primary)",
          fontSize: "var(--text-sm)",
          fontFamily: "var(--font-mono)",
          padding: "3px 6px",
        }}
        data-tooltip="Growing repeats the existing sequence cyclically; shrinking truncates from the end"
      />
      <button
        type="button"
        style={{
          ...btnStyle,
          opacity: resizeDraft.trim() === "" ? 0.4 : 1,
        }}
        onClick={applyResize}
        disabled={resizeDraft.trim() === ""}
      >
        Apply
      </button>
    </div>
  );
}
