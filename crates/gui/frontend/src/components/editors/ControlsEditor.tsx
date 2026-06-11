/* Controls editor — list of simple controls and rule-based controls.
   Edit text inline; toggle enabled. Mock-only persistence. */

import { useMemo, useState } from "react";
import { useAppState } from "../../AppContext";
import type { ControlEntry } from "../../hooks";

export function ControlsEditor({ accent }: { accent: string }) {
  const { showToast } = useAppState();
  const [entries, setEntries] = useState<ControlEntry[]>([]);
  const [filter, setFilter] = useState<"all" | "simple" | "rule">("all");
  const [search, setSearch] = useState("");

  const visible = useMemo(() => {
    return entries.filter((e) => {
      if (filter === "simple" && e.mode !== "simple") return false;
      if (filter === "rule" && e.mode !== "rule") return false;
      if (search) {
        const blob =
          e.mode === "simple"
            ? `${e.id} ${e.text}`
            : `${e.id} ${e.name} ${e.ifClause} ${e.thenClause} ${e.elseClause ?? ""}`;
        if (!blob.toLowerCase().includes(search.toLowerCase())) return false;
      }
      return true;
    });
  }, [entries, filter, search]);

  const counts = useMemo(
    () => ({
      all: entries.length,
      simple: entries.filter((e) => e.mode === "simple").length,
      rule: entries.filter((e) => e.mode === "rule").length,
    }),
    [entries],
  );

  function toggle(id: string) {
    setEntries((prev) =>
      prev.map((e) => (e.id === id ? { ...e, enabled: !e.enabled } : e)),
    );
  }
  function update(id: string, patch: Partial<ControlEntry>) {
    setEntries((prev) =>
      prev.map((e) => (e.id === id ? ({ ...e, ...patch } as ControlEntry) : e)),
    );
  }

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
      {/* Filter / search bar */}
      <div
        style={{
          height: 44,
          padding: "0 16px",
          borderBottom: "1px solid var(--border)",
          display: "flex",
          alignItems: "center",
          gap: 8,
          flexShrink: 0,
        }}
      >
        <Segmented
          value={filter}
          accent={accent}
          options={[
            { id: "all", label: `All · ${counts.all}` },
            { id: "simple", label: `Simple · ${counts.simple}` },
            { id: "rule", label: `Rules · ${counts.rule}` },
          ]}
          onChange={(v) => setFilter(v as any)}
        />
        <div style={{ flex: 1 }} />
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search…"
          style={{
            width: 220,
            height: 28,
            background: "var(--bg-card)",
            border: "1px solid var(--border)",
            borderRadius: 5,
            padding: "0 8px",
            color: "var(--text-primary)",
            fontFamily: "var(--font-ui)",
            fontSize: 12,
            outline: "none",
          }}
        />
        <button
          onClick={() => showToast("Feature coming soon")}
          style={{
            background: `${accent}26`,
            color: accent,
            border: `1px solid ${accent}55`,
            borderRadius: 5,
            padding: "0 10px",
            height: 28,
            fontSize: 12,
            fontFamily: "var(--font-ui)",
            cursor: "pointer",
          }}
        >
          + New control
        </button>
      </div>

      {/* Entry list */}
      <div
        style={{
          flex: 1,
          overflow: "auto",
          padding: 12,
          display: "flex",
          flexDirection: "column",
          gap: 8,
        }}
      >
        {visible.map((e) => (
          <ControlCard
            key={e.id}
            entry={e}
            accent={accent}
            onToggle={() => toggle(e.id)}
            onUpdate={(patch) => update(e.id, patch)}
          />
        ))}
        {visible.length === 0 && (
          <div
            style={{
              textAlign: "center",
              padding: 32,
              color: "var(--text-tertiary)",
              fontSize: 13,
            }}
          >
            No matching controls.
          </div>
        )}
      </div>
    </div>
  );
}

function Segmented({
  value,
  options,
  accent,
  onChange,
}: {
  value: string;
  options: { id: string; label: string }[];
  accent: string;
  onChange: (v: string) => void;
}) {
  return (
    <div
      style={{
        display: "inline-flex",
        border: "1px solid var(--border)",
        borderRadius: 5,
        overflow: "hidden",
        height: 28,
      }}
    >
      {options.map((o) => {
        const active = o.id === value;
        return (
          <button
            key={o.id}
            onClick={() => onChange(o.id)}
            style={{
              padding: "0 12px",
              border: "none",
              background: active ? `${accent}26` : "transparent",
              color: active ? accent : "var(--text-secondary)",
              fontSize: 12,
              fontFamily: "var(--font-ui)",
              cursor: "pointer",
              whiteSpace: "nowrap",
              borderRight: "1px solid var(--border)",
            }}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

function ControlCard({
  entry,
  accent,
  onToggle,
  onUpdate,
}: {
  entry: ControlEntry;
  accent: string;
  onToggle: () => void;
  onUpdate: (patch: Partial<ControlEntry>) => void;
}) {
  const isRule = entry.mode === "rule";
  return (
    <div
      style={{
        border: "1px solid var(--border)",
        borderRadius: 6,
        background: "var(--bg-card)",
        opacity: entry.enabled ? 1 : 0.55,
        transition: "opacity var(--t-fast)",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          padding: "8px 12px",
          borderBottom: "1px solid var(--border)",
        }}
      >
        <Toggle checked={entry.enabled} accent={accent} onChange={onToggle} />
        <span
          style={{
            fontFamily: "var(--font-mono)",
            fontSize: 12,
            color: "var(--text-primary)",
            fontWeight: 500,
          }}
        >
          {entry.id}
        </span>
        <span
          style={{
            fontSize: 10,
            padding: "2px 6px",
            background: isRule ? `${accent}26` : "var(--bg-rail)",
            color: isRule ? accent : "var(--text-tertiary)",
            borderRadius: 3,
            textTransform: "uppercase",
            letterSpacing: 0.4,
          }}
        >
          {entry.mode}
        </span>
        {isRule && (
          <span style={{ fontSize: 12, color: "var(--text-secondary)" }}>
            {entry.name}
          </span>
        )}
        {isRule && (
          <span
            style={{
              marginLeft: "auto",
              fontSize: 11,
              color: "var(--text-tertiary)",
            }}
          >
            priority{" "}
            <span
              style={{
                color: "var(--text-secondary)",
                fontFamily: "var(--font-mono)",
              }}
            >
              {entry.priority}
            </span>
          </span>
        )}
      </div>

      {!isRule ? (
        <div style={{ padding: 12 }}>
          <textarea
            value={entry.text}
            onChange={(ev) => onUpdate({ text: ev.target.value })}
            spellCheck={false}
            rows={1}
            style={{
              width: "100%",
              resize: "vertical",
              background: "var(--bg-input, var(--bg-app))",
              border: "1px solid var(--border)",
              borderRadius: 4,
              color: "var(--text-primary)",
              fontFamily: "var(--font-mono)",
              fontSize: 12,
              padding: "6px 8px",
              outline: "none",
            }}
          />
        </div>
      ) : (
        <div
          style={{
            padding: 12,
            display: "flex",
            flexDirection: "column",
            gap: 8,
          }}
        >
          <Clause
            label="IF"
            accent={accent}
            value={entry.ifClause}
            onChange={(v) => onUpdate({ ifClause: v })}
          />
          <Clause
            label="THEN"
            accent={accent}
            value={entry.thenClause}
            onChange={(v) => onUpdate({ thenClause: v })}
          />
          {entry.elseClause != null && (
            <Clause
              label="ELSE"
              accent={accent}
              value={entry.elseClause}
              onChange={(v) => onUpdate({ elseClause: v })}
            />
          )}
        </div>
      )}
    </div>
  );
}

function Clause({
  label,
  value,
  accent,
  onChange,
}: {
  label: string;
  value: string;
  accent: string;
  onChange: (v: string) => void;
}) {
  const lines = Math.max(1, value.split("\n").length);
  return (
    <div style={{ display: "flex", gap: 8 }}>
      <div
        style={{
          width: 48,
          fontSize: 10,
          fontWeight: 600,
          color: accent,
          fontFamily: "var(--font-mono)",
          textAlign: "right",
          paddingTop: 6,
          letterSpacing: 0.4,
        }}
      >
        {label}
      </div>
      <textarea
        value={value}
        onChange={(e) => onChange(e.target.value)}
        rows={lines}
        spellCheck={false}
        style={{
          flex: 1,
          resize: "vertical",
          background: "var(--bg-input, var(--bg-app))",
          border: "1px solid var(--border)",
          borderRadius: 4,
          color: "var(--text-primary)",
          fontFamily: "var(--font-mono)",
          fontSize: 12,
          padding: "6px 8px",
          outline: "none",
        }}
      />
    </div>
  );
}

function Toggle({
  checked,
  accent,
  onChange,
}: {
  checked: boolean;
  accent: string;
  onChange: () => void;
}) {
  return (
    <button
      onClick={onChange}
      data-tooltip={checked ? "Disable" : "Enable"}
      style={{
        width: 28,
        height: 16,
        padding: 0,
        border: "none",
        borderRadius: 8,
        background: checked ? accent : "var(--bg-rail)",
        cursor: "pointer",
        position: "relative",
        transition: "background var(--t-fast)",
        flexShrink: 0,
      }}
    >
      <span
        style={{
          position: "absolute",
          top: 2,
          left: checked ? 14 : 2,
          width: 12,
          height: 12,
          borderRadius: "50%",
          background: "var(--bg-app)",
          transition: "left var(--t-fast)",
        }}
      />
    </button>
  );
}
