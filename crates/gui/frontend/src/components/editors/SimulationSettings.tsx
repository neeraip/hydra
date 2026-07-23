import {
  ChevronDownIcon,
  ChevronRightIcon,
  PencilSquareIcon,
} from "@heroicons/react/16/solid";
import { useEffect, useMemo, useRef, useState } from "react";
import { useAppState } from "../../AppContext";
import {
  getSimParams,
  type SimParams,
  updateSimParams,
  useScenarios,
} from "../../hooks";
import { formatIpcError } from "../../hooks/ipc";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";
import {
  Empty,
  Field,
  FieldGrid,
  fmtClock,
  fmtHours,
  fmtMinutes,
  ghostBtn,
  HoursInput,
  MinutesInput,
  NumberInput,
  primaryBtn,
  Select,
  TextInput,
  TimeInput,
} from "./SimulationSettings/FormControls";

// ─────────────────────────────────────────────────────────────────────────────
// Project-level simulation settings.
//
// The base/model.inp [TIMES] and [OPTIONS] sections are the single source of
// truth. This panel reads them, lets the engineer edit a focused subset (with
// numerical knobs hidden behind a collapsible "Advanced" section), and writes
// changes back to disk. On save, every scenario INP is updated to match and
// every existing result is marked stale.
// ─────────────────────────────────────────────────────────────────────────────

export function SimulationSettings({ projectId }: { projectId: string }) {
  const { showToast, bumpSimParams } = useAppState();
  const { markEdited } = useNetworkVersion();
  const scenarios = useScenarios(projectId);
  const scenariosRef = useRef(scenarios);
  useEffect(() => {
    scenariosRef.current = scenarios;
  }, [scenarios]);

  const [original, setOriginal] = useState<SimParams | null>(null);
  const [draft, setDraft] = useState<SimParams | null>(null);
  const [editing, setEditing] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    getSimParams(projectId)
      .then((p) => {
        if (cancelled) return;
        setOriginal(p);
        setDraft(p);
        setEditing(false);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  const dirty = useMemo(() => {
    if (!original || !draft) return false;
    return JSON.stringify(original) !== JSON.stringify(draft);
  }, [original, draft]);

  if (error) {
    return <Empty>Could not read sim settings: {error}</Empty>;
  }
  if (original === null) {
    // null = no base INP yet, draft project. (loading state also lands here briefly.)
    return (
      <Empty>
        No base model yet. Import or create a model to configure simulation
        settings.
      </Empty>
    );
  }
  if (!draft) return null;

  function update<K extends keyof SimParams>(key: K, value: SimParams[K]) {
    setDraft((d) => (d ? { ...d, [key]: value } : d));
  }

  function cancel() {
    setDraft(original);
    setEditing(false);
  }

  async function save() {
    if (!draft) return;
    setSaving(true);
    try {
      await updateSimParams(projectId, draft);
      setOriginal(draft);
      setEditing(false);
      // Tell every `useSimParams` consumer (e.g. the canvas timeline) to
      // re-read [TIMES]/[OPTIONS] — this panel keeps its own local copy, but
      // without the bump other views kept the stale params until reload.
      bumpSimParams();
      // Mark base model and every scenario as stale so the Run button turns amber.
      markEdited(null);
      for (const s of scenariosRef.current) markEdited(s.id);
      showToast(
        "Simulation settings saved. Existing results marked stale.",
        "success",
      );
    } catch (err) {
      showToast(
        `Failed to save simulation settings: ${formatIpcError(err)}`,
        "error",
      );
    } finally {
      setSaving(false);
    }
  }

  return (
    <div>
      {/* Toolbar */}
      <div
        style={{
          display: "flex",
          justifyContent: "flex-end",
          alignItems: "center",
          gap: 8,
          marginBottom: 8,
        }}
      >
        {!editing ? (
          <button
            type="button"
            onClick={() => setEditing(true)}
            style={ghostBtn}
            data-tooltip="Edit simulation settings"
          >
            <PencilSquareIcon style={{ width: 12, height: 12 }} />
            Edit
          </button>
        ) : (
          <>
            <button
              type="button"
              onClick={cancel}
              disabled={saving}
              style={ghostBtn}
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={save}
              disabled={saving || !dirty}
              style={{
                ...primaryBtn,
                opacity: !saving && dirty ? 1 : 0.5,
                cursor: !saving && dirty ? "pointer" : "not-allowed",
              }}
            >
              {saving ? "Saving…" : "Save"}
            </button>
          </>
        )}
      </div>

      {/* Focused fields */}
      <FieldGrid>
        <Field
          label="Duration"
          help="Total simulation length"
          editing={editing}
          control={
            <HoursInput
              value={draft.duration}
              onChange={(s) => update("duration", s)}
            />
          }
          display={fmtHours(draft.duration)}
        />
        <Field
          label="Start clock"
          help="Wall-clock time at t=0"
          editing={editing}
          control={
            <TimeInput
              value={draft.startClocktime}
              onChange={(s) => update("startClocktime", s)}
            />
          }
          display={fmtClock(draft.startClocktime)}
        />
        <Field
          label="Hydraulic step"
          editing={editing}
          control={
            <MinutesInput
              value={draft.hydStep}
              onChange={(s) => update("hydStep", s)}
            />
          }
          display={fmtMinutes(draft.hydStep)}
        />
        <Field
          label="Pattern step"
          editing={editing}
          control={
            <MinutesInput
              value={draft.patternStep}
              onChange={(s) => update("patternStep", s)}
            />
          }
          display={fmtMinutes(draft.patternStep)}
        />
        <Field
          label="Report step"
          editing={editing}
          control={
            <MinutesInput
              value={draft.reportStep}
              onChange={(s) => update("reportStep", s)}
            />
          }
          display={fmtMinutes(draft.reportStep)}
        />
        <Field
          label="Headloss"
          editing={editing}
          control={
            <Select
              value={draft.headLossFormula}
              onChange={(v) =>
                update("headLossFormula", v as SimParams["headLossFormula"])
              }
              options={[
                { value: "H-W", label: "Hazen–Williams" },
                { value: "D-W", label: "Darcy–Weisbach" },
                { value: "C-M", label: "Chézy–Manning" },
              ]}
            />
          }
          display={
            draft.headLossFormula === "H-W"
              ? "Hazen–Williams"
              : draft.headLossFormula === "D-W"
                ? "Darcy–Weisbach"
                : "Chézy–Manning"
          }
        />
        <Field
          label="Demand model"
          editing={editing}
          control={
            <Select
              value={draft.demandModel}
              onChange={(v) =>
                update("demandModel", v as SimParams["demandModel"])
              }
              options={[
                { value: "DDA", label: "Demand-driven (DDA)" },
                { value: "PDA", label: "Pressure-driven (PDA)" },
              ]}
            />
          }
          display={
            draft.demandModel === "DDA" ? "Demand-driven" : "Pressure-driven"
          }
        />
        <Field
          label="Demand multiplier"
          help="Global scaling factor on base demands"
          editing={editing}
          control={
            <NumberInput
              value={draft.demandMultiplier}
              onChange={(v) => update("demandMultiplier", v)}
              step={0.05}
              min={0}
            />
          }
          display={draft.demandMultiplier.toFixed(3).replace(/\.?0+$/, "")}
        />
        <Field
          label="Quality"
          editing={editing}
          control={
            <Select
              value={draft.qualityMode}
              onChange={(v) =>
                update("qualityMode", v as SimParams["qualityMode"])
              }
              options={[
                { value: "none", label: "None" },
                { value: "chemical", label: "Chemical" },
                { value: "age", label: "Water age" },
                { value: "trace", label: "Source trace" },
              ]}
            />
          }
          display={
            draft.qualityMode === "none"
              ? "None"
              : draft.qualityMode === "chemical"
                ? `Chemical${draft.chemName ? ` (${draft.chemName})` : ""}`
                : draft.qualityMode === "age"
                  ? "Water age"
                  : `Source trace${draft.traceNode ? ` (${draft.traceNode})` : ""}`
          }
        />
        {draft.qualityMode === "trace" && (
          <Field
            label="Trace node"
            editing={editing}
            control={
              <TextInput
                value={draft.traceNode ?? ""}
                onChange={(v) => update("traceNode", v)}
                placeholder="Node ID"
              />
            }
            display={draft.traceNode ?? "—"}
          />
        )}
        {draft.demandModel === "PDA" && (
          <>
            <Field
              label="PDA min pressure"
              editing={editing}
              control={
                <NumberInput
                  value={draft.pdaMinPressure}
                  onChange={(v) => update("pdaMinPressure", v)}
                  step={1}
                  min={0}
                />
              }
              display={draft.pdaMinPressure.toString()}
            />
            <Field
              label="PDA req. pressure"
              editing={editing}
              control={
                <NumberInput
                  value={draft.pdaRequiredPressure}
                  onChange={(v) => update("pdaRequiredPressure", v)}
                  step={1}
                  min={0}
                />
              }
              display={draft.pdaRequiredPressure.toString()}
            />
            <Field
              label="PDA exponent"
              editing={editing}
              control={
                <NumberInput
                  value={draft.pdaPressureExponent}
                  onChange={(v) => update("pdaPressureExponent", v)}
                  step={0.05}
                  min={0}
                />
              }
              display={draft.pdaPressureExponent.toString()}
            />
          </>
        )}
      </FieldGrid>

      {/* Advanced */}
      <button
        type="button"
        onClick={() => setShowAdvanced((v) => !v)}
        style={{
          marginTop: 16,
          background: "transparent",
          border: "none",
          padding: 0,
          color: "var(--text-tertiary)",
          fontSize: 11,
          fontWeight: 600,
          letterSpacing: "0.05em",
          textTransform: "uppercase",
          fontFamily: "var(--font-ui)",
          cursor: "pointer",
          display: "inline-flex",
          alignItems: "center",
          gap: 4,
        }}
      >
        {showAdvanced ? (
          <ChevronDownIcon style={{ width: 12, height: 12 }} />
        ) : (
          <ChevronRightIcon style={{ width: 12, height: 12 }} />
        )}
        Advanced
      </button>
      {showAdvanced && (
        <div style={{ marginTop: 10 }}>
          <FieldGrid>
            <Field
              label="Max trials"
              help="Newton-Raphson iteration cap"
              editing={editing}
              control={
                <NumberInput
                  value={draft.maxIter}
                  onChange={(v) =>
                    update("maxIter", Math.max(1, Math.round(v)))
                  }
                  step={10}
                  min={1}
                />
              }
              display={draft.maxIter.toString()}
            />
            <Field
              label="Accuracy"
              help="Relative flow tolerance (Hacc)"
              editing={editing}
              control={
                <NumberInput
                  value={draft.flowTol}
                  onChange={(v) => update("flowTol", v)}
                  step={0.0001}
                  min={0}
                />
              }
              display={draft.flowTol.toString()}
            />
            <Field
              label="Head tolerance"
              editing={editing}
              control={
                <NumberInput
                  value={draft.headTol}
                  onChange={(v) => update("headTol", v)}
                  step={0.0001}
                  min={0}
                />
              }
              display={draft.headTol.toString()}
            />
            <Field
              label="Damp limit"
              editing={editing}
              control={
                <NumberInput
                  value={draft.dampLimit}
                  onChange={(v) => update("dampLimit", v)}
                  step={0.001}
                  min={0}
                />
              }
              display={draft.dampLimit.toString()}
            />
            <Field
              label="Status check freq."
              editing={editing}
              control={
                <NumberInput
                  value={draft.checkFreq}
                  onChange={(v) =>
                    update("checkFreq", Math.max(1, Math.round(v)))
                  }
                  step={1}
                  min={1}
                />
              }
              display={draft.checkFreq.toString()}
            />
            <Field
              label="Max status checks"
              editing={editing}
              control={
                <NumberInput
                  value={draft.maxCheck}
                  onChange={(v) =>
                    update("maxCheck", Math.max(1, Math.round(v)))
                  }
                  step={1}
                  min={1}
                />
              }
              display={draft.maxCheck.toString()}
            />
            <Field
              label="Viscosity"
              help="Kinematic viscosity (relative to water at 20 °C)"
              editing={editing}
              control={
                <NumberInput
                  value={draft.viscosity}
                  onChange={(v) => update("viscosity", v)}
                  step={1e-6}
                  min={0}
                />
              }
              display={draft.viscosity.toExponential(2)}
            />
            <Field
              label="Specific gravity"
              editing={editing}
              control={
                <NumberInput
                  value={draft.specificGravity}
                  onChange={(v) => update("specificGravity", v)}
                  step={0.01}
                  min={0}
                />
              }
              display={draft.specificGravity.toString()}
            />
            <Field
              label="Quality step"
              editing={editing}
              control={
                <MinutesInput
                  value={draft.qualStep}
                  onChange={(s) => update("qualStep", s)}
                />
              }
              display={fmtMinutes(draft.qualStep)}
            />
          </FieldGrid>
        </div>
      )}
    </div>
  );
}
