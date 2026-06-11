/* Time-pattern editor — 24-hour multiplier bars with editable values. */

import { useEffect, useMemo, useRef, useState } from "react";
import { useAppState } from "../../AppContext";
import { createPattern, type TimePattern, usePatterns } from "../../hooks";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";

export function PatternEditor({ accent }: { accent: string }) {
  const { showToast } = useAppState();
  const { bumpNetwork } = useNetworkVersion();
  const rawPatterns = usePatterns();
  const patterns = useMemo<TimePattern[]>(
    () =>
      rawPatterns.map((p) => ({
        id: p.id,
        label: p.id,
        multipliers: p.multipliers,
        stepHours: 1,
      })),
    [rawPatterns],
  );
  const [activeId, setActiveId] = useState<string | null>(null);
  const [overrides, setOverrides] = useState<Record<string, number[]>>({});
  const [creating, setCreating] = useState(false);
  const [newId, setNewId] = useState("");
  const [createError, setCreateError] = useState<string | null>(null);
  const newIdRef = useRef<HTMLInputElement | null>(null);

  useEffect(() => {
    if (creating) newIdRef.current?.focus();
  }, [creating]);

  const effectiveId = activeId ?? patterns[0]?.id ?? "";
  const pattern = patterns.find((p) => p.id === effectiveId) ?? null;
  const multipliers = pattern
    ? (overrides[effectiveId] ?? pattern.multipliers)
    : [];

  async function handleCreate() {
    const trimmed = newId.trim();
    if (!trimmed) {
      setCreateError("ID required");
      return;
    }
    try {
      await createPattern(trimmed);
      bumpNetwork();
      setActiveId(trimmed);
      setCreating(false);
      setNewId("");
      setCreateError(null);
    } catch (err) {
      setCreateError(
        typeof err === "string" ? err : "Failed to create pattern",
      );
    }
  }

  return (
    <div style={{ flex: 1, display: "flex", overflow: "hidden", minHeight: 0 }}>
      {/* Pattern list */}
      <div
        style={{
          width: 220,
          borderRight: "1px solid var(--border)",
          overflow: "auto",
          flexShrink: 0,
        }}
      >
        {patterns.map((p) => {
          const active = p.id === effectiveId;
          return (
            <button
              key={p.id}
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
              }}
            >
              <div
                style={{
                  fontSize: 13,
                  fontWeight: 500,
                  fontFamily: "var(--font-mono)",
                }}
              >
                {p.id}
              </div>
              <div
                style={{
                  fontSize: 11,
                  color: "var(--text-tertiary)",
                  marginTop: 2,
                }}
              >
                {p.label}
              </div>
            </button>
          );
        })}
        {creating ? (
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
                border: `1px solid ${createError ? "var(--danger)" : "var(--border-focus)"}`,
                borderRadius: 4,
                padding: "0 6px",
                color: "var(--text-primary)",
                fontFamily: "var(--font-mono)",
                fontSize: 12,
                outline: "none",
                boxSizing: "border-box",
              }}
            />
            {createError && (
              <div
                style={{ fontSize: 11, color: "var(--danger)", marginTop: 3 }}
              >
                {createError}
              </div>
            )}
            <div style={{ display: "flex", gap: 4, marginTop: 6 }}>
              <button
                onClick={handleCreate}
                style={{
                  flex: 1,
                  height: 24,
                  fontSize: 11,
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
                onClick={() => {
                  setCreating(false);
                  setNewId("");
                  setCreateError(null);
                }}
                style={{
                  flex: 1,
                  height: 24,
                  fontSize: 11,
                  background: "var(--bg-hover)",
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
            onClick={() => setCreating(true)}
            style={{
              width: "100%",
              padding: "10px 12px",
              border: "none",
              background: "transparent",
              color: "var(--text-tertiary)",
              cursor: "pointer",
              fontSize: 12,
              fontFamily: "var(--font-ui)",
              textAlign: "left",
            }}
          >
            + New pattern
          </button>
        )}
      </div>

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
                setOverrides({ ...overrides, [effectiveId]: next });
              }}
            />
            <PatternRow
              multipliers={multipliers}
              stepHours={pattern.stepHours}
              accent={accent}
              onChange={(idx, val) => {
                const next = [...multipliers];
                next[idx] = val;
                setOverrides({ ...overrides, [effectiveId]: next });
              }}
              onReset={() => {
                const { [effectiveId]: _, ...rest } = overrides;
                setOverrides(rest);
                showToast("Pattern reset to defaults");
              }}
              isOverridden={overrides[effectiveId] != null}
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
            fontSize: 13,
          }}
        >
          No time patterns defined. Use "+ New pattern" to create one.
        </div>
      )}
    </div>
  );
}

function PatternHeader({
  pattern,
  accent,
  multipliers,
}: {
  pattern: TimePattern;
  accent: string;
  multipliers: number[];
}) {
  const min = Math.min(...multipliers);
  const max = Math.max(...multipliers);
  const mean = multipliers.reduce((a, b) => a + b, 0) / multipliers.length;
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
      <div
        style={{
          fontSize: 16,
          fontWeight: 600,
          color: "var(--text-primary)",
          fontFamily: "var(--font-mono)",
        }}
      >
        {pattern.id}
      </div>
      <div style={{ fontSize: 13, color: "var(--text-secondary)" }}>
        {pattern.label}
      </div>
      <div
        style={{
          marginLeft: "auto",
          display: "flex",
          gap: 16,
          fontSize: 11,
          color: "var(--text-tertiary)",
        }}
      >
        <Stat label="Step" value={`${pattern.stepHours}h`} />
        <Stat label="Length" value={`${multipliers.length}`} />
        <Stat label="Min" value={min.toFixed(2)} accent={accent} />
        <Stat label="Mean" value={mean.toFixed(2)} accent={accent} />
        <Stat label="Max" value={max.toFixed(2)} accent={accent} />
      </div>
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
        style={{ fontSize: 9, textTransform: "uppercase", letterSpacing: 0.4 }}
      >
        {label}
      </span>
      <span
        style={{
          fontSize: 13,
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

  function handleMove(e: React.MouseEvent, idx: number) {
    if (dragIdx !== idx || !containerRef.current) return;
    const cell = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const rel = 1 - (e.clientY - cell.top) / cell.height;
    const val = Math.max(0, Math.min(yMax, rel * yMax));
    onChange(idx, parseFloat(val.toFixed(2)));
  }

  return (
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
        return (
          <div
            key={i}
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
                  fontSize: 10,
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
                  fontSize: 9,
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
  return (
    <div>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          marginBottom: 6,
          fontSize: 11,
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
            fontSize: 11,
            fontFamily: "var(--font-ui)",
            textTransform: "none",
            letterSpacing: 0,
          }}
        >
          Reset
        </button>
      </div>
      <div
        style={{
          display: "grid",
          gridTemplateColumns: `repeat(${Math.min(12, multipliers.length)}, minmax(60px, 1fr))`,
          gap: 6,
        }}
      >
        {multipliers.map((m, i) => (
          <label
            key={i}
            style={{ display: "flex", flexDirection: "column", gap: 2 }}
          >
            <span
              style={{
                fontSize: 9,
                color: "var(--text-tertiary)",
                fontFamily: "var(--font-mono)",
              }}
            >
              {(i * stepHours).toString().padStart(2, "0")}:00
            </span>
            <input
              type="number"
              step="0.05"
              value={m}
              onChange={(e) => {
                const v = parseFloat(e.target.value);
                if (!Number.isNaN(v)) onChange(i, v);
              }}
              style={{
                width: "100%",
                height: 26,
                background: "var(--bg-input, var(--bg-card))",
                border: "1px solid var(--border)",
                borderRadius: 4,
                color: "var(--text-primary)",
                fontSize: 12,
                fontFamily: "var(--font-mono)",
                padding: "0 6px",
                outline: "none",
              }}
            />
          </label>
        ))}
      </div>
    </div>
  );
}
