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
import type { GenericQuantity, Link, Node } from "../hooks";
import { UdsAnalysisView } from "./uds/AnalysisView";
import { UdsEditorView } from "./uds/EditorView";
import { UdsLinkInspectorBody } from "./uds/LinkInspectorBody";
import { UdsNodeInspectorBody } from "./uds/NodeInspectorBody";
import { UdsOverviewComposition } from "./uds/OverviewComposition";
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

/** Props of the Overview page's "Network" KPI grid. */
export interface OverviewCompositionProps {
  networkLoaded: boolean;
  fallbackNodeCount: number;
  fallbackLinkCount: number;
}

/** One engine-described result value for a selected element at the current
 * timeline step — label and unit are engine-authored (§6 catalog). */
export interface GenericElementValue {
  id: string;
  label: string;
  /** §5 quantity descriptor for the SI `value`; absent = dimensionless. */
  quantity?: GenericQuantity;
  /** SI value; `null`/`NaN` = not reported for this element. */
  value: number | null;
  /** Whether this is the canvas's active variable — the result card's big
   * value, mirroring the wds card's active-variable treatment. */
  primary?: boolean;
}

/** Props of the element inspector's node body. */
export interface NodeInspectorBodyProps {
  node: Node;
  onLocateLink: (id: string) => void;
  /** Current-period catalog values for this element, when the engine's
   * generic results are loaded; `null` before a run. */
  results?: GenericElementValue[] | null;
}

/** Props of the element inspector's link body. */
export interface LinkInspectorBodyProps {
  link: Link;
  onLocateNode: (id: string) => void;
  results?: GenericElementValue[] | null;
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
  /** The Overview page's "Network" KPI grid. Absent = the wds composition
   * (pipes/tanks/pumps with lengths and diameters). */
  OverviewComposition?: ComponentType<OverviewCompositionProps>;
  /** Element inspector bodies. Absent = the wds bodies (attribute tables +
   * pressure/flow result cards). */
  NodeInspectorBody?: ComponentType<NodeInspectorBodyProps>;
  LinkInspectorBody?: ComponentType<LinkInspectorBodyProps>;
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
  OverviewComposition: UdsOverviewComposition,
  NodeInspectorBody: UdsNodeInspectorBody,
  LinkInspectorBody: UdsLinkInspectorBody,
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
