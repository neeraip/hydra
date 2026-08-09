/** A criteria editor driven entirely by an engine's published catalog
 * (hydra-common §7.2): fields, labels, units, and defaults all arrive as
 * data, so this one component edits any engine's standard. Values are SI
 * in the valuation and edited in the active display system.
 *
 * The wds page still uses its bespoke `CriteriaEditor` — the canvas
 * shares that store — but any engine without such history starts here.
 */

import { useEffect, useState } from "react";
import { useUnitSystem } from "../../units";
import {
  type Criterion,
  criterionUnit,
  criterionValue,
  defaultValuation,
  fromDisplayValue,
  type QuantityInfo,
  toDisplayValue,
  type Valuation,
} from "./criteria";

export function CriteriaValuationEditor({
  catalog,
  values,
  onChange,
}: {
  catalog: Criterion[];
  values: Valuation;
  onChange: (next: Valuation) => void;
}) {
  const isDefault =
    JSON.stringify(values) === JSON.stringify(defaultValuation(catalog));
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
          The standard this network is assessed against.
        </span>
        {!isDefault && (
          <button
            type="button"
            onClick={() => onChange(defaultValuation(catalog))}
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
        {catalog.map((c) => (
          <CriterionField
            key={c.key}
            criterion={c}
            value={criterionValue(c, values)}
            onCommit={(v) => onChange({ ...values, [c.key]: v })}
          />
        ))}
      </div>
    </div>
  );
}

function CriterionField({
  criterion,
  value,
  onCommit,
}: {
  criterion: Criterion;
  value: number | number[];
  onCommit: (si: number | number[]) => void;
}) {
  const sys = useUnitSystem();
  const unit = criterionUnit(criterion.quantity, sys);
  return (
    <div title={criterion.help}>
      <div
        style={{
          fontSize: "var(--text-sm)",
          color: "var(--text-secondary)",
          marginBottom: 5,
        }}
      >
        {criterion.label}
        {unit ? ` (${unit})` : ""}
      </div>
      {typeof value === "number" ? (
        <SiNumberInput
          si={value}
          quantity={criterion.quantity}
          onCommit={(v) => onCommit(v)}
        />
      ) : (
        <div style={{ display: "flex", gap: 6 }}>
          {criterion.kind.type === "band" &&
            criterion.kind.cuts.map((cut, i) => (
              <div key={cut.key}>
                <SiNumberInput
                  si={value[i]}
                  quantity={criterion.quantity}
                  onCommit={(v) =>
                    onCommit(value.map((held, j) => (j === i ? v : held)))
                  }
                />
                <div
                  style={{
                    fontSize: "var(--text-xs)",
                    color: "var(--text-tertiary)",
                    marginTop: 2,
                  }}
                >
                  {cut.label}
                </div>
              </div>
            ))}
        </div>
      )}
    </div>
  );
}

/** One SI-backed number input edited in the active display system —
 * stepper clicks and typed digits commit live, invalid remnants revert
 * on blur, the same contract as the wds criteria inputs. */
function SiNumberInput({
  si,
  quantity,
  onCommit,
}: {
  si: number;
  quantity: QuantityInfo | undefined;
  onCommit: (si: number) => void;
}) {
  const sys = useUnitSystem();
  const asDisplay = (v: number) =>
    String(Number(toDisplayValue(v, quantity, sys).toFixed(2)));
  const [draft, setDraft] = useState(asDisplay(si));
  useEffect(() => {
    setDraft(String(Number(toDisplayValue(si, quantity, sys).toFixed(2))));
  }, [si, quantity, sys]);

  const commit = (next: string) => {
    const n = Number(next);
    if (next.trim() !== "" && Number.isFinite(n) && n >= 0) {
      onCommit(fromDisplayValue(n, quantity, sys));
      return true;
    }
    return false;
  };

  return (
    <input
      type="number"
      min={0}
      step={stepFor(toDisplayValue(si, quantity, sys))}
      value={draft}
      onChange={(e) => {
        setDraft(e.target.value);
        commit(e.target.value);
      }}
      onBlur={() => {
        if (!commit(draft)) setDraft(asDisplay(si));
      }}
      onKeyDown={(e) => {
        if (e.key === "Escape") setDraft(asDisplay(si));
      }}
      inputMode="decimal"
      style={{
        width: 62,
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

/** A stepper increment scaled to the value's magnitude, since a generic
 * editor cannot know a quantity's conventional step. */
function stepFor(display: number): number {
  const magnitude = Math.abs(display);
  if (magnitude >= 20) return 1;
  if (magnitude >= 2) return 0.5;
  return 0.1;
}
