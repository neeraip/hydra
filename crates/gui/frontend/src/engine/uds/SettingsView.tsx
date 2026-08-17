/** The uds implementation of the settings modal body.
 *
 * The run's timing is edited here — start, end, and the four steps — and
 * saved through the drainage engine's own reader and writer, base and
 * every scenario in lockstep, exactly the contract the wds body has. The
 * model-semantic choices (flow units, routing form, infiltration
 * relation) are shown read-only: each changes what other parts of the
 * model mean, so flipping one is a model edit, not a settings edit.
 *
 * This body used to be a read-only summary with a sentence saying
 * editing was "not available yet" — written before the editing contract
 * shipped, and never revisited after it did. A modeller whose run spans
 * no time (end equals start — the RESULTS-EMPTY case) was told to fix it
 * in a dialog that could not.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { useAppState } from "../../AppContext";
import {
  DateInput,
  Field,
  FieldGrid,
  MinutesInput,
  NumberInput,
  Select,
  TimeInput,
} from "../../components/editors/SimulationSettings/FormControls";
import { DialogButton } from "../../components/ui/DialogButton";
import {
  getUdsSimParams,
  type UdsSimParams,
  updateUdsSimParams,
} from "../../hooks";
import { fetchInto } from "../../hooks/fetchInto";
import { formatIpcError } from "../../hooks/ipc";
import { useNetworkVersion } from "../../hooks/NetworkVersionContext";
import { useScenarios } from "../../hooks/scenarios";
import type { SettingsViewProps } from "../registry";

function Group({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div style={{ marginBottom: 20 }}>
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
        {title}
      </div>
      <FieldGrid>{children}</FieldGrid>
    </div>
  );
}

export function UdsSettingsView({ projectId }: SettingsViewProps) {
  const { closeSimSettingsModal, showToast, bumpSimParams } = useAppState();
  const { markEdited } = useNetworkVersion();
  const scenarios = useScenarios(projectId);
  const scenariosRef = useRef(scenarios);
  useEffect(() => {
    scenariosRef.current = scenarios;
  }, [scenarios]);

  const [original, setOriginal] = useState<UdsSimParams | null>(null);
  const [draft, setDraft] = useState<UdsSimParams | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  // The backend's sentence, held beside the fields it is about — "the
  // run has to end after it starts" belongs under the end-time control,
  // not in a toast that slides away.
  const [refused, setRefused] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    return fetchInto(getUdsSimParams(projectId), (p) => {
      setOriginal(p);
      setDraft(p);
      setLoading(false);
    });
  }, [projectId]);

  const dirty = useMemo(
    () =>
      original != null &&
      draft != null &&
      JSON.stringify(original) !== JSON.stringify(draft),
    [original, draft],
  );

  function update<K extends keyof UdsSimParams>(
    key: K,
    value: UdsSimParams[K],
  ) {
    setDraft((d) => (d ? { ...d, [key]: value } : d));
  }

  async function save() {
    if (!draft || !dirty || saving) return;
    setSaving(true);
    setRefused(null);
    try {
      await updateUdsSimParams(projectId, draft);
      setOriginal(draft);
      // Tell every `useSimParams`-style consumer to re-read, and mark
      // base and every scenario stale so the Run button turns amber.
      bumpSimParams();
      markEdited(projectId, null);
      for (const s of scenariosRef.current) markEdited(projectId, s.id);
      showToast(
        "Simulation settings saved. Existing results marked stale.",
        "success",
      );
      closeSimSettingsModal();
    } catch (err) {
      setRefused(formatIpcError(err));
    } finally {
      setSaving(false);
    }
  }

  if (loading) {
    return (
      <div
        style={{ color: "var(--text-tertiary)", fontSize: "var(--text-md)" }}
      >
        Loading…
      </div>
    );
  }
  if (!draft) {
    return (
      <div
        style={{ color: "var(--text-tertiary)", fontSize: "var(--text-md)" }}
      >
        This project has no model yet. Import a model file to configure
        simulation settings.
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column" }}>
      <Group title="Model">
        <Field
          label="Flow units"
          help="The file's unit system. Values are converted, never reinterpreted."
          editing
          control={
            <Select
              value={draft.flowUnits}
              onChange={(v) => update("flowUnits", v)}
              options={[
                { value: "CFS", label: "CFS" },
                { value: "GPM", label: "GPM" },
                { value: "MGD", label: "MGD" },
                { value: "CMS", label: "CMS" },
                { value: "LPS", label: "LPS" },
                { value: "MLD", label: "MLD" },
              ]}
            />
          }
        />
        <Field
          label="Routing"
          editing
          control={
            <Select
              value={draft.routing}
              onChange={(v) => update("routing", v)}
              options={[
                { value: "STEADY", label: "Steady flow" },
                { value: "KINWAVE", label: "Kinematic wave" },
                { value: "DYNWAVE", label: "Dynamic wave" },
              ]}
            />
          }
        />
        <Field
          label="Infiltration"
          help="Movable within a parameter family; crossing families needs the subcatchments re-described first"
          editing
          control={
            <Select
              value={draft.infiltration}
              onChange={(v) => update("infiltration", v)}
              options={[
                { value: "HORTON", label: "Horton" },
                { value: "MODIFIED_HORTON", label: "Modified Horton" },
                { value: "GREEN_AMPT", label: "Green-Ampt" },
                { value: "MODIFIED_GREEN_AMPT", label: "Modified Green-Ampt" },
                { value: "CURVE_NUMBER", label: "Curve number" },
              ]}
            />
          }
        />
      </Group>

      <Group title="Timing">
        <Field
          label="Start date"
          editing
          control={
            <DateInput
              value={draft.startDate}
              onChange={(v) => update("startDate", v)}
            />
          }
        />
        <Field
          label="Start time"
          editing
          control={
            <TimeInput
              value={draft.startTime}
              onChange={(s) => update("startTime", s)}
            />
          }
        />
        <Field
          label="End date"
          editing
          control={
            <DateInput
              value={draft.endDate}
              onChange={(v) => update("endDate", v)}
            />
          }
        />
        <Field
          label="End time"
          editing
          control={
            <TimeInput
              value={draft.endTime}
              onChange={(s) => update("endTime", s)}
            />
          }
        />
        <Field
          label="Report step (min)"
          editing
          control={
            <MinutesInput
              value={draft.reportStep}
              onChange={(s) => update("reportStep", s)}
            />
          }
        />
        <Field
          label="Routing step (s)"
          help="Routing step cap; the solver may take shorter steps"
          editing
          control={
            <NumberInput
              value={draft.routingStep}
              onChange={(v) => update("routingStep", v)}
              step={1}
              min={0}
            />
          }
        />
        <Field
          label="Wet step (min)"
          help="Hydrology step during rainfall"
          editing
          control={
            <MinutesInput
              value={draft.wetStep}
              onChange={(s) => update("wetStep", s)}
            />
          }
        />
        <Field
          label="Dry step (min)"
          help="Hydrology step between storms"
          editing
          control={
            <MinutesInput
              value={draft.dryStep}
              onChange={(s) => update("dryStep", s)}
            />
          }
        />
      </Group>

      {refused && (
        <div
          style={{
            fontSize: "var(--text-sm)",
            color: "var(--danger)",
            marginBottom: 10,
          }}
        >
          {refused}
        </div>
      )}

      <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
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
        <DialogButton
          intent="primary"
          onClick={save}
          disabled={saving || !dirty}
          aria-label="Save simulation settings"
          data-tooltip={dirty ? "Save" : "No changes"}
        >
          {saving ? "Saving…" : "Save"}
        </DialogButton>
      </div>
    </div>
  );
}
