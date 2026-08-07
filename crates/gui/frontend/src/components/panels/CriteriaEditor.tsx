/**
 * The project's analysis criteria — the standard a network is assessed
 * against.
 *
 * These are engineering judgements about the network, not display settings,
 * and they have more than one consumer: the compliance figures on the
 * Analysis page, and the map's "Criteria" colour scale. They were once
 * authored in two unrelated surfaces, with the minimum service pressure on
 * one and the three bands inside the map legend's popover, so no screen ever
 * showed the whole ruler at once.
 *
 * Lives here rather than beside the Analysis page because it now has two
 * hosts: Analysis, where you read the compliance figures it produces, and a
 * panel over the canvas, where you can move a band and watch the map
 * recolour under it. One component and one store, because two editors for
 * one value is how the two halves of a setting come to disagree.
 */

import type React from "react";
import { useEffect, useState } from "react";
import { DEFAULT_CRITERIA, type ProjectCriteria } from "../../hooks";
import {
  fromDisplay,
  type Quantity,
  toDisplay,
  unitLabel,
  useUnitSystem,
} from "../../units";

/**
 * The project's analysis criteria, edited in one place.
 *
 * These are engineering judgements about the network — the standard it is
 * being assessed against — not display settings, and they have more than
 * one consumer: the compliance figures on this page, and the map's
 * "Criteria" colour scale. They used to be authored in two unrelated
 * surfaces, with the minimum service pressure here and the three bands
 * inside the map legend's popover, so no screen ever showed the whole
 * ruler at once.
 *
 * Values are stored in SI and edited in the active display system.
 */
export function CriteriaEditor({
  criteria,
  onChange,
}: {
  criteria: ProjectCriteria;
  onChange: (next: ProjectCriteria) => void;
}) {
  const isDefault =
    criteria.minPressureM === DEFAULT_CRITERIA.minPressureM &&
    (["pressure", "velocity", "flow"] as const).every((k) =>
      Object.keys(criteria[k]).every(
        (f) =>
          criteria[k][f as keyof (typeof criteria)[typeof k]] ===
          DEFAULT_CRITERIA[k][f as keyof (typeof DEFAULT_CRITERIA)[typeof k]],
      ),
    );

  return (
    <div style={{ fontFamily: "var(--font-ui)" }}>
      <div
        style={{
          display: "flex",
          alignItems: "baseline",
          gap: 10,
          marginBottom: 10,
        }}
      >
        <span
          style={{
            fontSize: "var(--text-sm)",
            fontWeight: 600,
            letterSpacing: "0.05em",
            textTransform: "uppercase",
            color: "var(--text-tertiary)",
          }}
        >
          Criteria
        </span>
        <span
          style={{ fontSize: "var(--text-sm)", color: "var(--text-tertiary)" }}
        >
          The standard this network is assessed against — used by the figures
          below and by the map's Criteria colour scale.
        </span>
        {!isDefault && (
          <button
            type="button"
            onClick={() => onChange({ ...DEFAULT_CRITERIA })}
            style={{
              background: "transparent",
              border: "none",
              color: "var(--accent)",
              fontSize: "var(--text-sm)",
              cursor: "pointer",
              fontFamily: "var(--font-ui)",
              marginLeft: "auto",
              flexShrink: 0,
            }}
          >
            Reset all
          </button>
        )}
      </div>

      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(auto-fit, minmax(230px, 1fr))",
          gap: "12px 20px",
        }}
      >
        <CriteriaField
          label="Min service pressure"
          quantity="pressure"
          value={criteria.minPressureM}
          onCommit={(v) => onChange({ ...criteria, minPressureM: v })}
        />
        <CriteriaBand
          label="Pressure"
          quantity="pressure"
          fields={["low", "required", "high"]}
          values={criteria.pressure}
          onCommit={(k, v) =>
            onChange({
              ...criteria,
              pressure: { ...criteria.pressure, [k]: v },
            })
          }
        />
        <CriteriaBand
          label="Velocity"
          quantity="velocity"
          fields={["low", "target", "high"]}
          values={criteria.velocity}
          onCommit={(k, v) =>
            onChange({
              ...criteria,
              velocity: { ...criteria.velocity, [k]: v },
            })
          }
        />
        <CriteriaBand
          label="Flow"
          quantity="flow"
          fields={["low", "target", "high"]}
          values={criteria.flow}
          onCommit={(k, v) =>
            onChange({ ...criteria, flow: { ...criteria.flow, [k]: v } })
          }
        />
      </div>
    </div>
  );
}

/** One SI-backed numeric input shown in the active display system. */
function CriteriaInput({
  value,
  quantity,
  onCommit,
  width = 62,
}: {
  value: number;
  quantity: Quantity;
  onCommit: (si: number) => void;
  width?: number;
}) {
  const sys = useUnitSystem();
  const asDisplay = (m: number) =>
    String(Number(toDisplay(m, quantity, sys).toFixed(2)));
  const [draft, setDraft] = useState(asDisplay(value));
  // Re-sync when the stored value or the display unit changes. Inlined so
  // the deps stay exhaustive.
  useEffect(() => {
    setDraft(String(Number(toDisplay(value, quantity, sys).toFixed(2))));
  }, [value, quantity, sys]);

  const commit = () => {
    const n = Number(draft);
    // A rejected edit reverts rather than storing NaN — a criterion that
    // silently became "not a number" would blank every figure derived from
    // it with no indication why.
    if (draft.trim() !== "" && Number.isFinite(n) && n >= 0) {
      onCommit(fromDisplay(n, quantity, sys));
    } else {
      setDraft(asDisplay(value));
    }
  };

  return (
    <input
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === "Enter") commit();
        else if (e.key === "Escape") setDraft(asDisplay(value));
      }}
      inputMode="decimal"
      style={{
        width,
        background: "var(--bg-input)",
        border: "1px solid var(--border)",
        borderRadius: 4,
        color: "var(--text-primary)",
        fontSize: "var(--text-md)",
        fontFamily: "var(--font-mono)",
        padding: "3px 6px",
        textAlign: "right",
        outline: "none",
      }}
    />
  );
}

/** A single labelled criterion. */
function CriteriaField({
  label,
  quantity,
  value,
  onCommit,
}: {
  label: string;
  quantity: Quantity;
  value: number;
  onCommit: (si: number) => void;
}) {
  const sys = useUnitSystem();
  return (
    <div>
      <div style={CRITERIA_LABEL_STYLE}>
        {label} ({unitLabel(quantity, sys)})
      </div>
      <CriteriaInput value={value} quantity={quantity} onCommit={onCommit} />
    </div>
  );
}

const CRITERIA_LABEL_STYLE: React.CSSProperties = {
  fontSize: "var(--text-sm)",
  color: "var(--text-secondary)",
  marginBottom: 5,
};

/** A three-value band: the cut points of one quantity's colour bands. */
function CriteriaBand<F extends string>({
  label,
  quantity,
  fields,
  values,
  onCommit,
}: {
  label: string;
  quantity: Quantity;
  fields: readonly F[];
  values: Record<F, number>;
  onCommit: (field: F, si: number) => void;
}) {
  const sys = useUnitSystem();
  return (
    <div>
      <div style={CRITERIA_LABEL_STYLE}>
        {label} ({unitLabel(quantity, sys)})
      </div>
      <div style={{ display: "flex", gap: 6 }}>
        {fields.map((f) => (
          <div key={f}>
            <CriteriaInput
              value={values[f]}
              quantity={quantity}
              onCommit={(v) => onCommit(f, v)}
              width={56}
            />
            <div
              style={{
                fontSize: "var(--text-xs)",
                color: "var(--text-tertiary)",
                textAlign: "center",
                marginTop: 2,
              }}
            >
              {f}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
