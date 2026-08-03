/**
 * The per-engine component registry: the GUI's design language expressed as
 * component interfaces, with a bespoke implementation per engine.
 *
 * The design concept is abstract — "the run modal shows a simulation
 * settings card with an edit affordance" — and each engine supplies the
 * component that fulfils it. Shared surfaces select an implementation from
 * this registry exactly once, keyed by the active project's engine; they
 * never branch on an engine key inside their bodies. A surface missing an
 * entry for an engine falls back to the wds implementation only where the
 * registry says so explicitly (`DEFAULT_ENGINE`), never silently.
 */

import type { ComponentType } from "react";
import { UdsAnalysisView } from "./uds/AnalysisView";
import { UdsEditorView } from "./uds/EditorView";
import { UdsRunSettingsSummary } from "./uds/RunSettingsSummary";
import { UdsSettingsView } from "./uds/SettingsView";
import { WdsRunSettingsSummary } from "./wds/RunSettingsSummary";

/** Props of the run modal's settings-card body. */
export interface RunSettingsSummaryProps {
  projectId: string;
}

/** Props of the settings modal's body for engines without the built-in
 * wds editor. */
export interface SettingsViewProps {
  projectId: string;
}

export interface EngineComponents {
  /** Body of the run modal's "Simulation settings" card. */
  RunSettingsSummary: ComponentType<RunSettingsSummaryProps>;
  /** Body of the settings modal. Absent = the shared wds editor owns it
   * (until it, too, moves behind this interface). */
  SettingsView?: ComponentType<SettingsViewProps>;
  /** Body of the Editor project view. Absent = the wds element editor. */
  EditorView?: ComponentType;
  /** Body of the Results project view. Absent = the wds analysis panels. */
  AnalysisView?: ComponentType;
  /** Whether the settings modal edits (true) or views (false). Drives the
   * edit affordance labels without any engine branching in the modals. */
  settingsEditable: boolean;
  /** Whether the model itself is editable in this GUI — canvas editing
   * tools, element tables, create modals. Read-only engines hide those
   * affordances entirely rather than offering gestures that refuse. */
  modelEditable: boolean;
}

const WDS: EngineComponents = {
  RunSettingsSummary: WdsRunSettingsSummary,
  settingsEditable: true,
  modelEditable: true,
};

const UDS: EngineComponents = {
  RunSettingsSummary: UdsRunSettingsSummary,
  SettingsView: UdsSettingsView,
  EditorView: UdsEditorView,
  AnalysisView: UdsAnalysisView,
  settingsEditable: false,
  modelEditable: false,
};

const REGISTRY: Record<string, EngineComponents> = {
  wds: WDS,
  uds: UDS,
};

const DEFAULT_ENGINE = WDS;

/** The component set for an engine key; wds for unknown/absent keys, which
 * matches every pre-engine-field project. */
export function engineComponents(
  key: string | null | undefined,
): EngineComponents {
  return (key != null ? REGISTRY[key] : undefined) ?? DEFAULT_ENGINE;
}
