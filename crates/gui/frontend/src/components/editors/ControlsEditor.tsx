/* Controls editor — simple (`[CONTROLS]`) and rule-based (`[RULES]`) controls.
   Edits are staged into the shared DraftContext, not committed to the
   backend immediately — they become part of the unified Network Editor
   draft alongside Elements/Curves/Patterns, saved or discarded together.
   Structured forms, not free-text editing, since the underlying engine
   data is structured. */

import { TrashIcon } from "@heroicons/react/16/solid";
import { useMemo, useState } from "react";
import {
  type Link,
  type Node,
  type RuleActionDto,
  type RuleDto,
  type RulePremiseAttribute,
  type RulePremiseDto,
  type SimpleControlDto,
  useControls,
  useLinks,
  useNodes,
  useRules,
} from "../../hooks";
import { useDraft } from "../../hooks/DraftContext";
import { DeleteConfirmModal } from "../modals/DeleteConfirmModal";

type ControlFilter = "all" | "simple" | "rule";

const inputStyle: React.CSSProperties = {
  height: 26,
  background: "var(--bg-input, var(--bg-card))",
  border: "1px solid var(--border)",
  borderRadius: 4,
  padding: "0 6px",
  color: "var(--text-primary)",
  fontFamily: "var(--font-ui)",
  fontSize: 12,
  outline: "none",
};

function settingUnitLabel(link: Link | undefined): string {
  if (link?.type !== "valve") return "";
  switch (link.valveType) {
    case "PRV":
    case "PSV":
    case "PBV":
      return "m";
    case "FCV":
      return "L/s";
    default:
      return "";
  }
}

function secondsToHhmm(s: number | null): string {
  if (s == null || !Number.isFinite(s)) return "00:00";
  const total = Math.max(0, Math.round(s));
  const h = Math.floor((total / 3600) % 24);
  const m = Math.floor((total % 3600) / 60);
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}`;
}

function hhmmToSeconds(v: string): number {
  const [h, m] = v.split(":").map((n) => parseInt(n, 10));
  return (
    (Number.isFinite(h) ? h : 0) * 3600 + (Number.isFinite(m) ? m : 0) * 60
  );
}

function defaultControl(links: Link[]): SimpleControlDto {
  return {
    linkId: links[0]?.id ?? "",
    actionStatus: "closed",
    actionSetting: null,
    triggerKind: "timer",
    triggerSeconds: 0,
    triggerNodeId: null,
    triggerValue: null,
    enabled: true,
  };
}

function defaultRule(): RuleDto {
  return {
    name: "",
    priority: 1,
    premises: [
      {
        object: "clock",
        nodeId: null,
        linkId: null,
        attribute: "time",
        operator: "ge",
        value: 0,
        statusValue: null,
        connective: null,
      },
    ],
    thenActions: [],
    elseActions: [],
  };
}

export function ControlsEditor({ accent }: { accent: string }) {
  const controls = useControls();
  const rules = useRules();
  const nodes = useNodes();
  const links = useLinks();
  const {
    controlAdds,
    setControlAdds,
    controlEdits,
    setControlEdits,
    controlDeletes,
    setControlDeletes,
    ruleAdds,
    setRuleAdds,
    ruleEdits,
    setRuleEdits,
    ruleDeletes,
    setRuleDeletes,
    nextTempKey,
  } = useDraft();

  // Merge staged creates/edits/deletes on top of the real lists so the UI
  // always reflects the current draft. Existing rows keep a stable
  // `idx-N` key (their current backend array position); new rows use a
  // `tmp-N` key until they're actually created on save.
  const mergedControls = useMemo(
    () => [
      ...controls
        .map((c, i) => ({
          key: `idx-${i}`,
          data: controlEdits.get(`idx-${i}`) ?? c,
        }))
        .filter(({ key }) => !controlDeletes.has(key)),
      ...Array.from(controlAdds.entries()).map(([key, data]) => ({
        key,
        data,
      })),
    ],
    [controls, controlEdits, controlDeletes, controlAdds],
  );
  const mergedRules = useMemo(
    () => [
      ...rules
        .map((r, i) => ({
          key: `idx-${i}`,
          data: ruleEdits.get(`idx-${i}`) ?? r,
        }))
        .filter(({ key }) => !ruleDeletes.has(key)),
      ...Array.from(ruleAdds.entries()).map(([key, data]) => ({ key, data })),
    ],
    [rules, ruleEdits, ruleDeletes, ruleAdds],
  );

  const [filter, setFilter] = useState<ControlFilter>("all");
  const [search, setSearch] = useState("");
  const [expandedControl, setExpandedControl] = useState<string | null>(null);
  const [expandedRule, setExpandedRule] = useState<string | null>(null);
  const [pendingDelete, setPendingDelete] = useState<{
    kind: "control" | "rule";
    key: string;
    label: string;
  } | null>(null);

  const controlSummaries = useMemo(
    () =>
      new Map(
        mergedControls.map(({ key, data }) => [key, summarizeControl(data)]),
      ),
    [mergedControls],
  );
  const ruleSummaries = useMemo(
    () =>
      new Map(mergedRules.map(({ key, data }) => [key, summarizeRule(data)])),
    [mergedRules],
  );

  const q = search.toLowerCase();
  const visibleControls = mergedControls.filter(
    ({ key }) =>
      !q || (controlSummaries.get(key) ?? "").toLowerCase().includes(q),
  );
  const visibleRules = mergedRules.filter(
    ({ key }) => !q || (ruleSummaries.get(key) ?? "").toLowerCase().includes(q),
  );

  const showControls = filter === "all" || filter === "simple";
  const showRules = filter === "all" || filter === "rule";

  function handleAddControl() {
    const key = nextTempKey("tmp-control-");
    setControlAdds((prev) => new Map(prev).set(key, defaultControl(links)));
    setExpandedControl(key);
  }

  function handleAddRule() {
    const key = nextTempKey("tmp-rule-");
    setRuleAdds((prev) => new Map(prev).set(key, defaultRule()));
    setExpandedRule(key);
  }

  function handleDelete() {
    if (!pendingDelete) return;
    const { kind, key } = pendingDelete;
    setPendingDelete(null);
    const isNew = key.startsWith("tmp-");
    if (kind === "control") {
      if (isNew) {
        setControlAdds((prev) => {
          const next = new Map(prev);
          next.delete(key);
          return next;
        });
      } else {
        setControlDeletes((prev) => new Set(prev).add(key));
        setControlEdits((prev) => {
          if (!prev.has(key)) return prev;
          const next = new Map(prev);
          next.delete(key);
          return next;
        });
      }
      if (expandedControl === key) setExpandedControl(null);
    } else {
      if (isNew) {
        setRuleAdds((prev) => {
          const next = new Map(prev);
          next.delete(key);
          return next;
        });
      } else {
        setRuleDeletes((prev) => new Set(prev).add(key));
        setRuleEdits((prev) => {
          if (!prev.has(key)) return prev;
          const next = new Map(prev);
          next.delete(key);
          return next;
        });
      }
      if (expandedRule === key) setExpandedRule(null);
    }
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
            {
              id: "all",
              label: `All · ${mergedControls.length + mergedRules.length}`,
            },
            { id: "simple", label: `Simple · ${mergedControls.length}` },
            { id: "rule", label: `Rules · ${mergedRules.length}` },
          ]}
          onChange={(v) => setFilter(v as ControlFilter)}
        />
        <div style={{ flex: 1 }} />
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search…"
          style={{ ...inputStyle, width: 220, height: 28 }}
        />
        <button
          type="button"
          onClick={handleAddControl}
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
        <button
          type="button"
          onClick={handleAddRule}
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
          + New rule
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
        {showControls &&
          visibleControls.map(({ key, data }) => (
            <ControlCard
              key={key}
              control={data}
              summary={controlSummaries.get(key) ?? ""}
              accent={accent}
              links={links}
              nodes={nodes}
              expanded={expandedControl === key}
              onToggleExpand={() =>
                setExpandedControl(expandedControl === key ? null : key)
              }
              onSave={(next) => {
                if (key.startsWith("tmp-")) {
                  setControlAdds((prev) => new Map(prev).set(key, next));
                } else {
                  setControlEdits((prev) => new Map(prev).set(key, next));
                }
              }}
              onDelete={() =>
                setPendingDelete({
                  kind: "control",
                  key,
                  label: `Control (${data.linkId})`,
                })
              }
            />
          ))}
        {showRules &&
          visibleRules.map(({ key, data }) => (
            <RuleCard
              key={key}
              rule={data}
              summary={ruleSummaries.get(key) ?? ""}
              accent={accent}
              links={links}
              nodes={nodes}
              expanded={expandedRule === key}
              onToggleExpand={() =>
                setExpandedRule(expandedRule === key ? null : key)
              }
              onSave={(next) => {
                if (key.startsWith("tmp-")) {
                  setRuleAdds((prev) => new Map(prev).set(key, next));
                } else {
                  setRuleEdits((prev) => new Map(prev).set(key, next));
                }
              }}
              onDelete={() =>
                setPendingDelete({ kind: "rule", key, label: data.name || key })
              }
            />
          ))}
        {mergedControls.length === 0 && mergedRules.length === 0 && (
          <div
            style={{
              textAlign: "center",
              padding: 32,
              color: "var(--text-tertiary)",
              fontSize: 13,
            }}
          >
            No controls defined. Use "+ New control" or "+ New rule" to create
            one.
          </div>
        )}
      </div>
      <DeleteConfirmModal
        open={pendingDelete != null}
        elementKind={pendingDelete?.kind ?? "control"}
        elementId={pendingDelete?.label ?? ""}
        onConfirm={handleDelete}
        onCancel={() => setPendingDelete(null)}
      />
    </div>
  );
}

function summarizeControl(c: SimpleControlDto): string {
  const action =
    c.actionStatus != null
      ? c.actionStatus.toUpperCase()
      : `SETTING ${c.actionSetting ?? ""}`;
  const trigger =
    c.triggerKind === "timer"
      ? `AT TIME ${secondsToHhmm(c.triggerSeconds)}`
      : c.triggerKind === "clocktime"
        ? `AT CLOCKTIME ${secondsToHhmm(c.triggerSeconds)}`
        : c.triggerKind === "hiLevel"
          ? `IF ${c.triggerNodeId ?? "?"} ABOVE ${c.triggerValue ?? "?"} m`
          : `IF ${c.triggerNodeId ?? "?"} BELOW ${c.triggerValue ?? "?"} m`;
  return `LINK ${c.linkId} ${action} ${trigger}`;
}

function premiseSummary(p: RulePremiseDto): string {
  const obj =
    p.object === "clock"
      ? "CLOCK"
      : p.object === "node"
        ? (p.nodeId ?? "?")
        : (p.linkId ?? "?");
  const val = p.attribute === "status" ? (p.statusValue ?? "?") : p.value;
  return `${obj} ${p.attribute} ${p.operator} ${val}`;
}

function summarizeRule(r: RuleDto): string {
  const ifPart = r.premises.map(premiseSummary).join(" ");
  return `${r.name}: IF ${ifPart}`;
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
            type="button"
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

function CardShell({
  accent,
  headline,
  badge,
  expanded,
  onToggleExpand,
  onDelete,
  children,
}: {
  accent: string;
  headline: string;
  badge: string;
  expanded: boolean;
  onToggleExpand: () => void;
  onDelete: () => void;
  children: React.ReactNode;
}) {
  return (
    <div
      style={{
        border: "1px solid var(--border)",
        borderRadius: 6,
        background: "var(--bg-card)",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          padding: "8px 12px",
          borderBottom: expanded ? "1px solid var(--border)" : "none",
        }}
      >
        <button
          type="button"
          onClick={onToggleExpand}
          style={{
            flex: 1,
            minWidth: 0,
            display: "flex",
            alignItems: "center",
            gap: 10,
            background: "none",
            border: "none",
            cursor: "pointer",
            textAlign: "left",
            padding: 0,
          }}
        >
          <span
            style={{
              fontSize: 10,
              padding: "2px 6px",
              background: `${accent}26`,
              color: accent,
              borderRadius: 3,
              textTransform: "uppercase",
              letterSpacing: 0.4,
              flexShrink: 0,
            }}
          >
            {badge}
          </span>
          <span
            style={{
              fontFamily: "var(--font-mono)",
              fontSize: 12,
              color: "var(--text-primary)",
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {headline}
          </span>
        </button>
        <button
          type="button"
          onClick={onDelete}
          title="Delete"
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
          <TrashIcon style={{ width: 13, height: 13 }} />
        </button>
      </div>
      {expanded && <div style={{ padding: 12 }}>{children}</div>}
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
      <span style={{ fontSize: 10, color: "var(--text-tertiary)" }}>
        {label}
      </span>
      {children}
    </div>
  );
}

// ── Simple control card ─────────────────────────────────────────────────────

function ControlCard({
  control,
  summary,
  accent,
  links,
  nodes,
  expanded,
  onToggleExpand,
  onSave,
  onDelete,
}: {
  control: SimpleControlDto;
  summary: string;
  accent: string;
  links: Link[];
  nodes: Node[];
  expanded: boolean;
  onToggleExpand: () => void;
  onSave: (next: SimpleControlDto) => void;
  onDelete: () => void;
}) {
  const [draft, setDraft] = useState<SimpleControlDto>(control);

  const link = links.find((l) => l.id === draft.linkId);
  const unit = settingUnitLabel(link);

  // Stage every change into the shared draft immediately (matching
  // Curves/Patterns) — no separate per-card Save button. `onSave` is called
  // as a plain statement, not from inside `setDraft`'s updater — updater
  // callbacks must stay pure; triggering another component's setState from
  // inside one caused the summary to sometimes go stale under batching.
  function sync(next: Partial<SimpleControlDto>) {
    const merged = { ...draft, ...next };
    setDraft(merged);
    onSave(merged);
  }

  return (
    <CardShell
      accent={accent}
      headline={summary}
      badge="simple"
      expanded={expanded}
      onToggleExpand={() => {
        setDraft(control);
        onToggleExpand();
      }}
      onDelete={onDelete}
    >
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(3, 1fr)",
          gap: 10,
        }}
      >
        <Field label="Link">
          <select
            value={draft.linkId}
            onChange={(e) => sync({ linkId: e.target.value })}
            style={inputStyle}
          >
            {links.map((l) => (
              <option key={l.id} value={l.id}>
                {l.id}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Action">
          <select
            value={draft.actionStatus != null ? "status" : "setting"}
            onChange={(e) =>
              sync(
                e.target.value === "status"
                  ? { actionStatus: "closed", actionSetting: null }
                  : { actionStatus: null, actionSetting: 0 },
              )
            }
            style={inputStyle}
          >
            <option value="status">Status</option>
            <option value="setting">Setting</option>
          </select>
        </Field>
        {draft.actionStatus != null ? (
          <Field label="Status">
            <select
              value={draft.actionStatus}
              onChange={(e) =>
                sync({ actionStatus: e.target.value as "open" | "closed" })
              }
              style={inputStyle}
            >
              <option value="open">Open</option>
              <option value="closed">Closed</option>
            </select>
          </Field>
        ) : (
          <Field label={`Setting${unit ? ` (${unit})` : ""}`}>
            <input
              type="number"
              value={draft.actionSetting ?? 0}
              onChange={(e) =>
                sync({ actionSetting: parseFloat(e.target.value) })
              }
              style={inputStyle}
            />
          </Field>
        )}
        <Field label="Trigger">
          <select
            value={draft.triggerKind}
            onChange={(e) => {
              const kind = e.target.value as SimpleControlDto["triggerKind"];
              sync(
                kind === "timer" || kind === "clocktime"
                  ? {
                      triggerKind: kind,
                      triggerSeconds: draft.triggerSeconds ?? 0,
                      triggerNodeId: null,
                      triggerValue: null,
                    }
                  : {
                      triggerKind: kind,
                      triggerSeconds: null,
                      triggerNodeId:
                        draft.triggerNodeId ?? nodes[0]?.id ?? null,
                      triggerValue: draft.triggerValue ?? 0,
                    },
              );
            }}
            style={inputStyle}
          >
            <option value="timer">At time (elapsed)</option>
            <option value="clocktime">At clock time</option>
            <option value="hiLevel">If node above</option>
            <option value="loLevel">If node below</option>
          </select>
        </Field>
        {draft.triggerKind === "timer" || draft.triggerKind === "clocktime" ? (
          <Field label="Time (HH:MM)">
            <input
              type="time"
              value={secondsToHhmm(draft.triggerSeconds)}
              onChange={(e) =>
                sync({ triggerSeconds: hhmmToSeconds(e.target.value) })
              }
              style={inputStyle}
            />
          </Field>
        ) : (
          <>
            <Field label="Node">
              <select
                value={draft.triggerNodeId ?? ""}
                onChange={(e) => sync({ triggerNodeId: e.target.value })}
                style={inputStyle}
              >
                {nodes.map((n) => (
                  <option key={n.id} value={n.id}>
                    {n.id}
                  </option>
                ))}
              </select>
            </Field>
            <Field label="Threshold (m)">
              <input
                type="number"
                value={draft.triggerValue ?? 0}
                onChange={(e) =>
                  sync({ triggerValue: parseFloat(e.target.value) })
                }
                style={inputStyle}
              />
            </Field>
          </>
        )}
      </div>
    </CardShell>
  );
}

// ── Rule card ────────────────────────────────────────────────────────────────

const NODE_ATTRS: RulePremiseAttribute[] = [
  "head",
  "pressure",
  "demand",
  "level",
  "fillTime",
  "drainTime",
];
const LINK_ATTRS: RulePremiseAttribute[] = [
  "flow",
  "status",
  "setting",
  "power",
];
const CLOCK_ATTRS: RulePremiseAttribute[] = ["time", "clockTime"];

function attrsForObject(
  object: RulePremiseDto["object"],
): RulePremiseAttribute[] {
  if (object === "node") return NODE_ATTRS;
  if (object === "link") return LINK_ATTRS;
  return CLOCK_ATTRS;
}

function PremiseRow({
  premise,
  isLast,
  nodes,
  links,
  onChange,
  onRemove,
}: {
  premise: RulePremiseDto;
  isLast: boolean;
  nodes: Node[];
  links: Link[];
  onChange: (next: RulePremiseDto) => void;
  onRemove: () => void;
}) {
  const attrs = attrsForObject(premise.object);
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        flexWrap: "wrap",
        padding: "6px 0",
        borderBottom: "1px solid var(--border)",
      }}
    >
      <select
        value={premise.object}
        onChange={(e) => {
          const object = e.target.value as RulePremiseDto["object"];
          onChange({
            ...premise,
            object,
            nodeId: object === "node" ? (nodes[0]?.id ?? null) : null,
            linkId: object === "link" ? (links[0]?.id ?? null) : null,
            attribute: attrsForObject(object)[0],
          });
        }}
        style={inputStyle}
      >
        <option value="node">Node</option>
        <option value="link">Link</option>
        <option value="clock">Clock</option>
      </select>
      {premise.object === "node" && (
        <select
          value={premise.nodeId ?? ""}
          onChange={(e) => onChange({ ...premise, nodeId: e.target.value })}
          style={inputStyle}
        >
          {nodes.map((n) => (
            <option key={n.id} value={n.id}>
              {n.id}
            </option>
          ))}
        </select>
      )}
      {premise.object === "link" && (
        <select
          value={premise.linkId ?? ""}
          onChange={(e) => onChange({ ...premise, linkId: e.target.value })}
          style={inputStyle}
        >
          {links.map((l) => (
            <option key={l.id} value={l.id}>
              {l.id}
            </option>
          ))}
        </select>
      )}
      <select
        value={premise.attribute}
        onChange={(e) =>
          onChange({
            ...premise,
            attribute: e.target.value as RulePremiseAttribute,
          })
        }
        style={inputStyle}
      >
        {attrs.map((a) => (
          <option key={a} value={a}>
            {a}
          </option>
        ))}
      </select>
      <select
        value={premise.operator}
        onChange={(e) =>
          onChange({
            ...premise,
            operator: e.target.value as RulePremiseDto["operator"],
          })
        }
        style={inputStyle}
      >
        <option value="eq">=</option>
        <option value="neq">≠</option>
        <option value="lt">&lt;</option>
        <option value="gt">&gt;</option>
        <option value="le">≤</option>
        <option value="ge">≥</option>
      </select>
      {premise.attribute === "status" ? (
        <select
          value={premise.statusValue ?? "open"}
          onChange={(e) =>
            onChange({
              ...premise,
              statusValue: e.target.value as "open" | "closed" | "active",
            })
          }
          style={inputStyle}
        >
          <option value="open">Open</option>
          <option value="closed">Closed</option>
          <option value="active">Active</option>
        </select>
      ) : (
        <input
          type="number"
          value={premise.value}
          onChange={(e) =>
            onChange({ ...premise, value: parseFloat(e.target.value) })
          }
          style={{ ...inputStyle, width: 80 }}
        />
      )}
      {!isLast && (
        <select
          value={premise.connective ?? "and"}
          onChange={(e) =>
            onChange({ ...premise, connective: e.target.value as "and" | "or" })
          }
          style={inputStyle}
        >
          <option value="and">AND</option>
          <option value="or">OR</option>
        </select>
      )}
      <button
        type="button"
        onClick={onRemove}
        style={{
          background: "none",
          border: "none",
          color: "var(--text-tertiary)",
          cursor: "pointer",
          marginLeft: "auto",
        }}
      >
        <TrashIcon style={{ width: 12, height: 12 }} />
      </button>
    </div>
  );
}

function ActionRow({
  action,
  links,
  onChange,
  onRemove,
}: {
  action: RuleActionDto;
  links: Link[];
  onChange: (next: RuleActionDto) => void;
  onRemove: () => void;
}) {
  const link = links.find((l) => l.id === action.linkId);
  const unit = settingUnitLabel(link);
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        flexWrap: "wrap",
        padding: "6px 0",
        borderBottom: "1px solid var(--border)",
      }}
    >
      <select
        value={action.linkId}
        onChange={(e) => onChange({ ...action, linkId: e.target.value })}
        style={inputStyle}
      >
        {links.map((l) => (
          <option key={l.id} value={l.id}>
            {l.id}
          </option>
        ))}
      </select>
      <select
        value={action.status != null ? "status" : "setting"}
        onChange={(e) =>
          onChange(
            e.target.value === "status"
              ? { ...action, status: "open", setting: null }
              : { ...action, status: null, setting: 0 },
          )
        }
        style={inputStyle}
      >
        <option value="status">Status</option>
        <option value="setting">Setting</option>
      </select>
      {action.status != null ? (
        <select
          value={action.status}
          onChange={(e) =>
            onChange({ ...action, status: e.target.value as "open" | "closed" })
          }
          style={inputStyle}
        >
          <option value="open">Open</option>
          <option value="closed">Closed</option>
        </select>
      ) : (
        <input
          type="number"
          value={action.setting ?? 0}
          onChange={(e) =>
            onChange({ ...action, setting: parseFloat(e.target.value) })
          }
          style={{ ...inputStyle, width: 80 }}
          placeholder={unit || undefined}
        />
      )}
      {unit && action.setting != null && (
        <span style={{ fontSize: 10, color: "var(--text-tertiary)" }}>
          {unit}
        </span>
      )}
      <button
        type="button"
        onClick={onRemove}
        style={{
          background: "none",
          border: "none",
          color: "var(--text-tertiary)",
          cursor: "pointer",
          marginLeft: "auto",
        }}
      >
        <TrashIcon style={{ width: 12, height: 12 }} />
      </button>
    </div>
  );
}

function RuleCard({
  rule,
  summary,
  accent,
  links,
  nodes,
  expanded,
  onToggleExpand,
  onSave,
  onDelete,
}: {
  rule: RuleDto;
  summary: string;
  accent: string;
  links: Link[];
  nodes: Node[];
  expanded: boolean;
  onToggleExpand: () => void;
  onSave: (next: RuleDto) => void;
  onDelete: () => void;
}) {
  const [draft, setDraft] = useState<RuleDto>(rule);

  // Stage every change into the shared draft immediately (matching
  // Curves/Patterns) — no separate per-card Save button. `onSave` is called
  // as a plain statement, not from inside `setDraft`'s updater — updater
  // callbacks must stay pure; triggering another component's setState from
  // inside one caused the summary to sometimes go stale under batching.
  function update(fn: (d: RuleDto) => RuleDto) {
    const next = fn(draft);
    setDraft(next);
    onSave(next);
  }

  function addPremise() {
    update((d) => ({
      ...d,
      premises: [
        ...d.premises,
        {
          object: "clock",
          nodeId: null,
          linkId: null,
          attribute: "time",
          operator: "ge",
          value: 0,
          statusValue: null,
          connective: null,
        },
      ],
    }));
  }
  function addAction(which: "thenActions" | "elseActions") {
    if (links.length === 0) return;
    update((d) => ({
      ...d,
      [which]: [
        ...d[which],
        { linkId: links[0].id, status: "open", setting: null },
      ],
    }));
  }

  return (
    <CardShell
      accent={accent}
      headline={summary}
      badge="rule"
      expanded={expanded}
      onToggleExpand={() => {
        setDraft(rule);
        onToggleExpand();
      }}
      onDelete={onDelete}
    >
      <div
        style={{
          display: "flex",
          gap: 12,
          alignItems: "center",
          marginBottom: 10,
        }}
      >
        <Field label="Name">
          <input
            value={draft.name}
            disabled
            style={{ ...inputStyle, width: 80, opacity: 0.6 }}
          />
        </Field>
        <Field label="Priority">
          <input
            type="number"
            value={draft.priority}
            onChange={(e) =>
              update((d) => ({ ...d, priority: parseFloat(e.target.value) }))
            }
            style={{ ...inputStyle, width: 70 }}
          />
        </Field>
      </div>

      <div
        style={{ fontSize: 10, color: "var(--text-tertiary)", marginBottom: 4 }}
      >
        IF
      </div>
      {draft.premises.map((p, i) => (
        <PremiseRow
          // biome-ignore lint/suspicious/noArrayIndexKey: premises have no stable id; rows are edited/added/removed in place.
          key={i}
          premise={p}
          isLast={i === draft.premises.length - 1}
          nodes={nodes}
          links={links}
          onChange={(next) =>
            update((d) => ({
              ...d,
              premises: d.premises.map((pp, pi) => (pi === i ? next : pp)),
            }))
          }
          onRemove={() =>
            update((d) => ({
              ...d,
              premises: d.premises.filter((_, pi) => pi !== i),
            }))
          }
        />
      ))}
      <button
        type="button"
        onClick={addPremise}
        style={{
          background: "none",
          border: "none",
          color: accent,
          cursor: "pointer",
          fontSize: 11,
          padding: "6px 0",
        }}
      >
        + Add premise
      </button>

      <div
        style={{
          fontSize: 10,
          color: "var(--text-tertiary)",
          margin: "10px 0 4px",
        }}
      >
        THEN
      </div>
      {draft.thenActions.map((a, i) => (
        <ActionRow
          // biome-ignore lint/suspicious/noArrayIndexKey: actions have no stable id; rows are edited/added/removed in place.
          key={i}
          action={a}
          links={links}
          onChange={(next) =>
            update((d) => ({
              ...d,
              thenActions: d.thenActions.map((aa, ai) =>
                ai === i ? next : aa,
              ),
            }))
          }
          onRemove={() =>
            update((d) => ({
              ...d,
              thenActions: d.thenActions.filter((_, ai) => ai !== i),
            }))
          }
        />
      ))}
      <button
        type="button"
        onClick={() => addAction("thenActions")}
        style={{
          background: "none",
          border: "none",
          color: accent,
          cursor: "pointer",
          fontSize: 11,
          padding: "6px 0",
        }}
      >
        + Add THEN action
      </button>

      <div
        style={{
          fontSize: 10,
          color: "var(--text-tertiary)",
          margin: "10px 0 4px",
        }}
      >
        ELSE
      </div>
      {draft.elseActions.map((a, i) => (
        <ActionRow
          // biome-ignore lint/suspicious/noArrayIndexKey: actions have no stable id; rows are edited/added/removed in place.
          key={i}
          action={a}
          links={links}
          onChange={(next) =>
            update((d) => ({
              ...d,
              elseActions: d.elseActions.map((aa, ai) =>
                ai === i ? next : aa,
              ),
            }))
          }
          onRemove={() =>
            update((d) => ({
              ...d,
              elseActions: d.elseActions.filter((_, ai) => ai !== i),
            }))
          }
        />
      ))}
      <button
        type="button"
        onClick={() => addAction("elseActions")}
        style={{
          background: "none",
          border: "none",
          color: accent,
          cursor: "pointer",
          fontSize: 11,
          padding: "6px 0",
        }}
      >
        + Add ELSE action
      </button>
    </CardShell>
  );
}
