import { Cog6ToothIcon, XMarkIcon } from "@heroicons/react/16/solid";
import { useEffect, useMemo, useRef, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { engineComponents } from "../../engine/registry";
import {
  ACCENT,
  getSimParams,
  type SimParams,
  updateSimParams,
  useScenarios,
} from "../../hooks";
import { formatIpcError } from "../../hooks/ipc";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";
import { primaryModifierLabel, primaryModifierPressed } from "../../shortcuts";
import { fromDisplay, toDisplay, unitLabel, useUnitSystem } from "../../units";
import {
  Empty,
  Field,
  FieldGrid,
  HoursInput,
  MinutesInput,
  NumberInput,
  Select,
  TextInput,
  TimeInput,
} from "../editors/SimulationSettings/FormControls";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

// ─────────────────────────────────────────────────────────────────────────────
// Simulation settings modal.
//
// The single editor for the model's global [TIMES]/[OPTIONS]/[ENERGY]/report
// configuration. Reached from the gear on the Simulate split-button (persistent
// on every project view) and from the Run modal's "Edit settings" link — so it
// is page-independent without occupying permanent screen space.
//
// The base/model.inp is the single source of truth. On save every scenario INP
// is rewritten to match and every existing result is marked stale.
// ─────────────────────────────────────────────────────────────────────────────

function SubHeading({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        fontSize: "var(--text-sm)",
        fontWeight: 700,
        letterSpacing: "0.06em",
        textTransform: "uppercase",
        color: "var(--text-tertiary)",
        marginBottom: 10,
      }}
    >
      {children}
    </div>
  );
}

function SettingsGroup({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div style={{ marginBottom: 20 }}>
      <SubHeading>{title}</SubHeading>
      <FieldGrid>{children}</FieldGrid>
    </div>
  );
}

export function SimSettingsModal() {
  const sys = useUnitSystem();
  const {
    simSettingsModalOpen,
    closeSimSettingsModal,
    activeProjectId,
    showToast,
    bumpSimParams,
  } = useAppState();
  const { project, engine } = useActiveProject();
  const { markEdited } = useNetworkVersion();
  const scenarios = useScenarios(activeProjectId ?? null);
  const scenariosRef = useRef(scenarios);
  useEffect(() => {
    scenariosRef.current = scenarios;
  }, [scenarios]);

  const [original, setOriginal] = useState<SimParams | null>(null);
  const [draft, setDraft] = useState<SimParams | null>(null);
  // Engines with their own settings body supply it via the registry; this
  // modal owns only the chrome. `SettingsView` absent = the editor below.
  const components = engineComponents(engine?.key);
  const SettingsView = components.SettingsView;
  const [loadError, setLoadError] = useState<string | null>(null);
  // Distinct from `original === null`, which also means "no model on disk" —
  // without this the modal claims to be loading forever on an empty project.
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);

  // (Re)load params whenever the modal opens or the project changes — the model
  // may have been edited elsewhere since the last open. Engines with their
  // own settings body fetch their own data; the wds params IPC would only
  // resolve null for them.
  useEffect(() => {
    if (!simSettingsModalOpen || !activeProjectId || SettingsView) return;
    let cancelled = false;
    setLoadError(null);
    setOriginal(null);
    setDraft(null);
    setLoading(true);
    getSimParams(activeProjectId)
      .then((p) => {
        if (cancelled) return;
        setOriginal(p);
        setDraft(p);
      })
      .catch((e) => {
        if (!cancelled) setLoadError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [simSettingsModalOpen, activeProjectId, SettingsView]);

  const dirty = useMemo(() => {
    if (!original || !draft) return false;
    return JSON.stringify(original) !== JSON.stringify(draft);
  }, [original, draft]);

  async function save() {
    if (!draft || !activeProjectId || !dirty) return;
    setSaving(true);
    try {
      await updateSimParams(activeProjectId, draft);
      setOriginal(draft);
      // Tell every `useSimParams` consumer (e.g. the canvas timeline) to
      // re-read [TIMES]/[OPTIONS].
      bumpSimParams();
      // Mark base and every scenario stale so the Run button turns amber.
      markEdited(activeProjectId, null);
      for (const s of scenariosRef.current) markEdited(activeProjectId, s.id);
      showToast(
        "Simulation settings saved. Existing results marked stale.",
        "success",
      );
      closeSimSettingsModal();
    } catch (err) {
      showToast(
        `Failed to save simulation settings: ${formatIpcError(err)}`,
        "error",
      );
    } finally {
      setSaving(false);
    }
  }

  // Esc closes; Cmd/Ctrl+Enter saves.
  const saveRef = useRef(save);
  saveRef.current = save;
  useEffect(() => {
    if (!simSettingsModalOpen) return;
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") {
        e.preventDefault();
        closeSimSettingsModal();
      }
      if (primaryModifierPressed(e) && e.key === "Enter") {
        e.preventDefault();
        void saveRef.current();
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [simSettingsModalOpen, closeSimSettingsModal]);

  if (!simSettingsModalOpen) return null;

  function update<K extends keyof SimParams>(key: K, value: SimParams[K]) {
    setDraft((d) => (d ? { ...d, [key]: value } : d));
  }

  const saveHint = `${primaryModifierLabel()}↵`;

  return (
    <ModalBackdrop
      onDismiss={closeSimSettingsModal}
      zIndex={200}
      style={{ animation: "fadeIn 120ms ease-out" }}
    >
      <div
        {...stopBackdropEvents}
        style={{
          width: "100%",
          maxWidth: 640,
          maxHeight: "86vh",
          background: "var(--bg-panel)",
          backdropFilter: "blur(24px)",
          border: "1px solid var(--border-hover)",
          borderRadius: 12,
          boxShadow: "var(--shadow-3)",
          overflow: "hidden",
          display: "flex",
          flexDirection: "column",
          animation: "scaleIn 160ms ease-out",
        }}
      >
        {/* Header */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 12,
            padding: "14px 20px",
            borderBottom: "1px solid var(--border)",
          }}
        >
          <span
            style={{
              fontSize: "var(--text-sm)",
              fontWeight: 700,
              letterSpacing: "0.06em",
              color: ACCENT,
              background: "var(--accent-dim)",
              border: "1px solid var(--selection-border)",
              padding: "3px 8px",
              borderRadius: 4,
            }}
          >
            {engine?.pill ?? "??"}
          </span>
          <div style={{ flex: 1 }}>
            <div
              style={{
                fontSize: "var(--text-xl)",
                fontWeight: 600,
                color: "var(--text-primary)",
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
              }}
            >
              <Cog6ToothIcon style={{ width: 14, height: 14 }} />
              Simulation Settings
            </div>
            <div
              style={{
                fontSize: "var(--text-md)",
                color: "var(--text-tertiary)",
              }}
            >
              {project?.name ?? "(no project)"}
            </div>
          </div>
          <button
            type="button"
            className="tl-btn"
            onClick={closeSimSettingsModal}
            data-tooltip="Close (Esc)"
            style={{
              width: 26,
              height: 26,
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
            }}
          >
            <XMarkIcon style={{ width: 14, height: 14 }} />
          </button>
        </div>

        {/* Body */}
        <div style={{ flex: 1, overflowY: "auto", padding: "18px 20px" }}>
          {SettingsView != null && activeProjectId != null ? (
            <SettingsView projectId={activeProjectId} />
          ) : loadError ? (
            <Empty>Could not read simulation settings: {loadError}</Empty>
          ) : original === null || draft === null ? (
            <Empty>
              {!activeProjectId
                ? "No project selected."
                : loading
                  ? "Loading…"
                  : "This project has no network yet. Import a model file or build one in the editor to configure simulation settings."}
            </Empty>
          ) : (
            <SettingsBody draft={draft} update={update} sys={sys} />
          )}
        </div>

        {/* Footer */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 12,
            padding: "12px 20px",
            borderTop: "1px solid var(--border)",
            background: "rgba(0,0,0,0.18)",
          }}
        >
          <div
            style={{
              flex: 1,
              fontSize: "var(--text-sm)",
              color: "var(--text-tertiary)",
            }}
          >
            {dirty
              ? "Saving rewrites every scenario and marks results stale."
              : ""}
          </div>
          <button
            type="button"
            onClick={closeSimSettingsModal}
            style={{
              background: "transparent",
              border: "1px solid var(--border)",
              color: "var(--text-secondary)",
              borderRadius: 5,
              padding: "7px 14px",
              fontSize: "var(--text-md)",
              cursor: "pointer",
              fontFamily: "var(--font-ui)",
            }}
          >
            {components.settingsEditable ? "Cancel" : "Close"}
          </button>
          {components.settingsEditable && (
            <button
              type="button"
              onClick={save}
              disabled={saving || !dirty}
              data-tooltip={dirty ? `Save (${saveHint})` : "No changes"}
              style={{
                background: !saving && dirty ? ACCENT : "var(--bg-card)",
                border: `1px solid ${!saving && dirty ? ACCENT : "var(--border)"}`,
                color: !saving && dirty ? "#fff" : "var(--text-disabled)",
                borderRadius: 5,
                padding: "7px 16px",
                fontSize: "var(--text-md)",
                fontWeight: 600,
                cursor: !saving && dirty ? "pointer" : "not-allowed",
                opacity: !saving && dirty ? 1 : 0.6,
                fontFamily: "var(--font-ui)",
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
              }}
            >
              {saving ? "Saving…" : "Save"}
              <span
                style={{
                  fontSize: "var(--text-xs)",
                  opacity: 0.85,
                  fontFamily: "var(--font-mono)",
                }}
              >
                {saveHint}
              </span>
            </button>
          )}
        </div>
      </div>
    </ModalBackdrop>
  );
}

// ── Body ──────────────────────────────────────────────────────────────────────

function SettingsBody({
  draft,
  update,
  sys,
}: {
  draft: SimParams;
  update: <K extends keyof SimParams>(key: K, value: SimParams[K]) => void;
  sys: ReturnType<typeof useUnitSystem>;
}) {
  return (
    <>
      <SettingsGroup title="Timing">
        <Field
          label="Duration"
          help="Total simulation length"
          editing
          control={
            <HoursInput
              value={draft.duration}
              onChange={(s) => update("duration", s)}
            />
          }
        />
        <Field
          label="Start clock"
          help="Wall-clock time at t=0"
          editing
          control={
            <TimeInput
              value={draft.startClocktime}
              onChange={(s) => update("startClocktime", s)}
            />
          }
        />
        <Field
          label="Hydraulic step"
          editing
          control={
            <MinutesInput
              value={draft.hydStep}
              onChange={(s) => update("hydStep", s)}
            />
          }
        />
        <Field
          label="Pattern step"
          editing
          control={
            <MinutesInput
              value={draft.patternStep}
              onChange={(s) => update("patternStep", s)}
            />
          }
        />
        <Field
          label="Report step"
          editing
          control={
            <MinutesInput
              value={draft.reportStep}
              onChange={(s) => update("reportStep", s)}
            />
          }
        />
      </SettingsGroup>

      <SettingsGroup title="Hydraulics">
        <Field
          label="Headloss"
          editing
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
        />
        <Field
          label="Demand model"
          editing
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
        />
        <Field
          label="Demand multiplier"
          help="Global scaling factor on base demands"
          editing
          control={
            <NumberInput
              value={draft.demandMultiplier}
              onChange={(v) => update("demandMultiplier", v)}
              step={0.05}
              min={0}
            />
          }
        />
        {draft.demandModel === "PDA" && (
          <>
            <Field
              label={`PDA min pressure (${unitLabel("pressure", sys)})`}
              editing
              control={
                <NumberInput
                  value={Number(
                    toDisplay(draft.pdaMinPressure, "pressure", sys).toFixed(2),
                  )}
                  onChange={(v) =>
                    update("pdaMinPressure", fromDisplay(v, "pressure", sys))
                  }
                  step={1}
                  min={0}
                />
              }
            />
            <Field
              label={`PDA req. pressure (${unitLabel("pressure", sys)})`}
              editing
              control={
                <NumberInput
                  value={Number(
                    toDisplay(
                      draft.pdaRequiredPressure,
                      "pressure",
                      sys,
                    ).toFixed(2),
                  )}
                  onChange={(v) =>
                    update(
                      "pdaRequiredPressure",
                      fromDisplay(v, "pressure", sys),
                    )
                  }
                  step={1}
                  min={0}
                />
              }
            />
            <Field
              label="PDA exponent"
              editing
              control={
                <NumberInput
                  value={draft.pdaPressureExponent}
                  onChange={(v) => update("pdaPressureExponent", v)}
                  step={0.05}
                  min={0}
                />
              }
            />
          </>
        )}
      </SettingsGroup>

      <SettingsGroup title="Water quality">
        <Field
          label="Quality"
          editing
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
        />
        {draft.qualityMode === "trace" && (
          <Field
            label="Trace node"
            editing
            control={
              <TextInput
                value={draft.traceNode ?? ""}
                onChange={(v) => update("traceNode", v)}
                placeholder="Node ID"
              />
            }
          />
        )}
        {draft.qualityMode === "chemical" && (
          <>
            <Field
              label="Chemical name"
              editing
              control={
                <TextInput
                  value={draft.chemName}
                  onChange={(v) => update("chemName", v)}
                  placeholder="e.g. Chlorine"
                />
              }
            />
            <Field
              label="Chemical units"
              editing
              control={
                <TextInput
                  value={draft.chemUnits}
                  onChange={(v) => update("chemUnits", v)}
                  placeholder="e.g. mg/L"
                />
              }
            />
          </>
        )}
      </SettingsGroup>

      <SettingsGroup title="Energy">
        <Field
          label="Global pump efficiency (%)"
          help="Default efficiency for pumps without an efficiency curve"
          editing
          control={
            <NumberInput
              value={draft.energyEfficiency}
              onChange={(v) =>
                update("energyEfficiency", Math.min(100, Math.max(0, v)))
              }
              step={1}
              min={0}
              max={100}
            />
          }
        />
        <Field
          label="Energy price ($/kWh)"
          editing
          control={
            <NumberInput
              value={draft.energyPrice}
              onChange={(v) => update("energyPrice", Math.max(0, v))}
              step={0.01}
              min={0}
            />
          }
        />
        <Field
          label="Price pattern"
          help="Pattern ID modulating price over time (optional)"
          editing
          control={
            <TextInput
              value={draft.energyPricePattern ?? ""}
              onChange={(v) =>
                update("energyPricePattern", v.trim() === "" ? null : v)
              }
              placeholder="Pattern ID"
            />
          }
        />
        <Field
          label="Demand charge ($/kW)"
          help="Added charge on peak power usage"
          editing
          control={
            <NumberInput
              value={draft.peakDemandCharge}
              onChange={(v) => update("peakDemandCharge", Math.max(0, v))}
              step={0.1}
              min={0}
            />
          }
        />
      </SettingsGroup>

      <SettingsGroup title="Reporting">
        <Field
          label="Time statistic"
          help="How reported values aggregate across the run"
          editing
          control={
            <Select
              value={draft.statistic}
              onChange={(v) => update("statistic", v as SimParams["statistic"])}
              options={[
                { value: "series", label: "Time series (none)" },
                { value: "average", label: "Average" },
                { value: "minimum", label: "Minimum" },
                { value: "maximum", label: "Maximum" },
                { value: "range", label: "Range (max − min)" },
              ]}
            />
          }
        />
      </SettingsGroup>

      <SettingsGroup title="Advanced (numerical)">
        <Field
          label="Max trials"
          help="Newton-Raphson iteration cap"
          editing
          control={
            <NumberInput
              value={draft.maxIter}
              onChange={(v) => update("maxIter", Math.max(1, Math.round(v)))}
              step={10}
              min={1}
            />
          }
        />
        <Field
          label="Accuracy"
          help="Relative flow tolerance (Hacc)"
          editing
          control={
            <NumberInput
              value={draft.flowTol}
              onChange={(v) => update("flowTol", v)}
              step={0.0001}
              min={0}
            />
          }
        />
        <Field
          label="Head tolerance"
          editing
          control={
            <NumberInput
              value={draft.headTol}
              onChange={(v) => update("headTol", v)}
              step={0.0001}
              min={0}
            />
          }
        />
        <Field
          label="Damp limit"
          editing
          control={
            <NumberInput
              value={draft.dampLimit}
              onChange={(v) => update("dampLimit", v)}
              step={0.001}
              min={0}
            />
          }
        />
        <Field
          label="Status check freq."
          editing
          control={
            <NumberInput
              value={draft.checkFreq}
              onChange={(v) => update("checkFreq", Math.max(1, Math.round(v)))}
              step={1}
              min={1}
            />
          }
        />
        <Field
          label="Max status checks"
          editing
          control={
            <NumberInput
              value={draft.maxCheck}
              onChange={(v) => update("maxCheck", Math.max(1, Math.round(v)))}
              step={1}
              min={1}
            />
          }
        />
        <Field
          label="Viscosity"
          help="Kinematic viscosity (relative to water at 20 °C)"
          editing
          control={
            <NumberInput
              value={draft.viscosity}
              onChange={(v) => update("viscosity", v)}
              step={1e-6}
              min={0}
            />
          }
        />
        <Field
          label="Specific gravity"
          editing
          control={
            <NumberInput
              value={draft.specificGravity}
              onChange={(v) => update("specificGravity", v)}
              step={0.01}
              min={0}
            />
          }
        />
        <Field
          label="Quality step"
          editing
          control={
            <MinutesInput
              value={draft.qualStep}
              onChange={(s) => update("qualStep", s)}
            />
          }
        />
      </SettingsGroup>
    </>
  );
}
