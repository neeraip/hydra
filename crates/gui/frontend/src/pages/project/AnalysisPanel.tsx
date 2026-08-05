import { useEffect, useState } from "react";
import { useActiveProject, useAppState, useSimulation } from "../../AppContext";
import { MetricChip } from "../../components/ui/MetricChip";
import { WarningRow } from "../../components/ui/WarningRow";
import {
  DEFAULT_CRITERIA,
  getResultAnalytics,
  type ProjectCriteria,
  type PumpEnergyRecord,
  type ResultAnalytics,
  useProjectCriteria,
} from "../../hooks";
import {
  formatQty,
  fromDisplay,
  type Quantity,
  toDisplay,
  unitLabel,
  useUnitSystem,
} from "../../units";
import { AuditPanels } from "./AnalysisPanel/AuditPanels";
import { pressureCompliancePct } from "./AnalysisPanel/compliance";
import {
  PressureHistogram,
  VelocityHistogram,
} from "./AnalysisPanel/Histograms";
import { PipeCriticality } from "./AnalysisPanel/PipeCriticality";
import { PumpEnergyPanel } from "./AnalysisPanel/PumpEnergyPanel";
import { TankLevelsPanel } from "./AnalysisPanel/TankLevelsPanel";
import { WorstNodesPanel } from "./AnalysisPanel/WorstNodesPanel";

export function AnalysisPanel() {
  const { resultMeta, pumpEnergy } = useSimulation();
  const { project } = useActiveProject();
  const { activeScenarioId, deferredProjectView } = useAppState();
  const visible = deferredProjectView === "analysis";
  // The compliance criterion is a property of the project, not of the app:
  // it was previously a single global value, so reviewing one network against
  // a 30 m standard silently recomputed every other network's compliance.
  const { criteria, setCriteria } = useProjectCriteria(project?.id ?? null);
  const minPressure = criteria.minPressureM;

  // Load analytics from the backend — streams the .out file one period at a
  // time so it is safe for arbitrarily large networks.  Re-fetches whenever
  // the result changes (resultMeta changes on every new run).
  const [analytics, setAnalytics] = useState<ResultAnalytics | null>(null);
  useEffect(() => {
    if (!project?.id || !resultMeta) {
      setAnalytics(null);
      return;
    }
    // Gated on visibility: the panel stays mounted while hidden, and this
    // fetch streams the whole .out file server-side — running it during a
    // tab/scenario switch contended with the switch's own IPC. On becoming
    // visible the effect re-runs and fetches (analytics may be one result
    // behind while hidden, which is fine — nothing displays it).
    if (!visible) return;
    let cancelled = false;
    getResultAnalytics(project.id, activeScenarioId, minPressure)
      .then((a) => {
        if (!cancelled) setAnalytics(a);
      })
      .catch((err) => {
        // Fall back to the empty-state placeholders ("—" metrics).
        console.error("Failed to load result analytics:", err);
        if (!cancelled) setAnalytics(null);
      });
    return () => {
      cancelled = true;
    };
  }, [project?.id, activeScenarioId, resultMeta, visible, minPressure]);

  return (
    <div
      style={{
        padding: 24,
        display: "flex",
        flexDirection: "column",
        gap: 20,
        animation: "fadeIn 150ms ease-out",
      }}
    >
      {/* Criteria editor — every band in one place */}
      <CriteriaEditor criteria={criteria} onChange={setCriteria} />

      {/* Panel 1: System Summary */}
      <SystemSummary
        analytics={analytics}
        pumpEnergy={pumpEnergy}
        minPressureM={minPressure}
      />

      {/* Panel 2: Two-column histograms */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 20 }}>
        <PressureHistogram analytics={analytics} minPressureM={minPressure} />
        <VelocityHistogram analytics={analytics} />
      </div>

      {/* Panel 3: Worst offenders — links (velocity) and nodes (pressure) */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 20 }}>
        <PipeCriticality analytics={analytics} />
        <WorstNodesPanel analytics={analytics} minPressureM={minPressure} />
      </div>

      {/* Panel 4: Mass-balance & energy audit */}
      <AuditPanels
        analytics={analytics}
        periodCount={analytics?.periodCount ?? null}
      />

      {/* Panel 5: Tank head trends */}
      <TankLevelsPanel analytics={analytics} />

      {/* Panel 6: Pump Energy */}
      <PumpEnergyPanel />
    </div>
  );
}

/* ── System Summary ──────────────────────────────────────────────────────────── */

/**
 * Total pump energy over the run as a chip value, or "—" when pump energy
 * hasn't loaded (or predates the backend's `totalKwh` field).
 */
function pumpEnergyChipValue(pumpEnergy: PumpEnergyRecord[] | null): string {
  if (!pumpEnergy || pumpEnergy.length === 0) return "—";
  let total = 0;
  let hasKwh = false;
  for (const p of pumpEnergy) {
    if (typeof p.totalKwh === "number" && Number.isFinite(p.totalKwh)) {
      total += p.totalKwh;
      hasKwh = true;
    }
  }
  if (!hasKwh) return "—";
  return `${total >= 100 ? total.toFixed(0) : total.toFixed(1)} kWh`;
}

function SystemSummary({
  analytics,
  pumpEnergy,
  minPressureM,
}: {
  analytics: ResultAnalytics | null;
  pumpEnergy: PumpEnergyRecord[] | null;
  minPressureM: number;
}) {
  const sys = useUnitSystem();
  if (!analytics) {
    return (
      <div>
        <div style={{ display: "flex", gap: 12, marginBottom: 12 }}>
          <MetricChip value="—" label="Min Pressure" />
          <MetricChip value="—" label="Max Velocity" />
          <MetricChip value="—" label="Pump Energy" />
          <MetricChip value="—" label="Mass Balance" />
        </div>
        <WarningRow>Run a simulation to see real system metrics.</WarningRow>
      </div>
    );
  }

  const compliancePct = pressureCompliancePct(analytics);

  // Min-pressure / max-velocity markers are absent when no valid data exists
  // in the results (e.g. no junctions, or no links with velocity data).
  const hasMinPressure = analytics.minPressureM != null;
  const minPressureColor =
    analytics.minPressureM != null && analytics.minPressureM < minPressureM
      ? "var(--status-error)"
      : undefined;

  return (
    <div>
      <div style={{ display: "flex", gap: 12, marginBottom: 12 }}>
        <MetricChip
          value={
            analytics.minPressureM != null
              ? formatQty(analytics.minPressureM, "pressure", sys, 1)
              : "—"
          }
          label={
            hasMinPressure && analytics.minPressureNodeId != null
              ? `Min Pressure (${analytics.minPressureNodeId})`
              : "Min Pressure"
          }
          valueColor={minPressureColor}
        />
        <MetricChip
          value={
            analytics.maxVelocityMs != null
              ? formatQty(analytics.maxVelocityMs, "velocity", sys, 2)
              : "—"
          }
          label={
            analytics.maxVelocityMs != null &&
            analytics.maxVelocityLinkId != null
              ? `Max Velocity (${analytics.maxVelocityLinkId})`
              : "Max Velocity"
          }
        />
        {compliancePct != null && (
          <MetricChip
            value={`${compliancePct.toFixed(1)} %`}
            label={`Pressure ≥ ${formatQty(minPressureM, "pressure", sys, sys === "si" ? 0 : 1)}`}
            valueColor={
              compliancePct < 100 ? "var(--status-warning)" : undefined
            }
          />
        )}
        <MetricChip
          value={pumpEnergyChipValue(pumpEnergy)}
          label="Pump Energy"
        />
        <MetricChip
          value={`${analytics.massBalance.balancePct.toFixed(1)} %`}
          label="Mass Balance"
        />
      </div>
      {analytics.lowPressureCount > 0 && (
        <WarningRow>
          {analytics.lowPressureCount} junction
          {analytics.lowPressureCount > 1 ? "s" : ""} below the minimum pressure
          criterion of{" "}
          {formatQty(minPressureM, "pressure", sys, sys === "si" ? 0 : 1)} at
          peak demand.
        </WarningRow>
      )}
    </div>
  );
}

/* ── Criteria editor ─────────────────────────────────────────────────────────── */

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
function CriteriaEditor({
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
