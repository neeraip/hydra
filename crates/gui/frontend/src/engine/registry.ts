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
import { ANIMATED_LINK_VARIABLES } from "../canvas/linkPulse";
import type { GenericQuantity, Link, Node } from "../hooks";
import type { Region } from "../types/network";
import { UdsAnalysisView } from "./uds/AnalysisView";
import { UdsElementsView } from "./uds/ElementsView";
import { UdsLinkInspectorBody } from "./uds/LinkInspectorBody";
import { UdsNodeInspectorBody } from "./uds/NodeInspectorBody";
import { UdsOverviewComposition } from "./uds/OverviewComposition";
import { UdsRegionInspectorBody } from "./uds/RegionInspectorBody";
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
  /** Select an areal element that drains to this node. Absent for engines
   * with no areal elements. */
  onLocateRegion?: (id: string) => void;
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

/** Props of the element inspector's areal-element body. */
export interface RegionInspectorBodyProps {
  region: Region;
  /** Select the element this region discharges to. */
  onLocateOutlet: (id: string) => void;
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
  /** Body of the areal-element inspector. Absent = the engine has no
   * areal elements and the canvas never selects one. */
  RegionInspectorBody?: ComponentType<RegionInspectorBodyProps>;
  /** Whether this engine's Editor view can receive and reveal a focused
   * element (the inspector's "Open in editor" affordance). False hides the
   * button instead of navigating to a view that ignores the request. */
  editorFocusesElements: boolean;
  /** Whether the settings modal edits (true) or views (false). Drives the
   * edit affordance labels without any engine branching in the modals. */
  settingsEditable: boolean;
  /** Whether the model itself is editable in this GUI — canvas editing
   * tools, element tables, create modals. Read-only engines hide those
   * affordances entirely rather than offering gestures that refuse. */
  modelEditable: boolean;
  /**
   * Result-variable ids this engine's projects hold criteria bands for.
   *
   * The project criteria file is a water-distribution compliance standard
   * (minimum service pressure and pressure/velocity/flow bands), so its
   * bands mean something only for the engine they were designed for.
   * Matching on variable id alone is not enough: two engines can publish a
   * variable called `flow` and mean different quantities by it, and a
   * drainage map was briefly offered a Criteria scale annotated with
   * water-distribution numbers.
   *
   * Empty means this engine has no such standard, and the legend never
   * offers the option.
   */
  criteriaVariables: readonly string[];
  /**
   * Result-variable ids whose motion the canvas can animate.
   *
   * The pulse reads a link's flow to decide how fast to move and what the
   * movement is claiming, so what it can animate depends on what an engine
   * publishes and on what that variable means. Matching by id alone would
   * not do: `flow` and `velocity` happen to be spelled the same in both
   * engines today, and the sentence offered to a reader whose selection is
   * not animated was the water distribution one for everybody — a drainage
   * map named Unit headloss and Quality, which drainage does not have.
   *
   * Empty means the engine has no animated variables and the toggle stays
   * inert wherever it appears.
   */
  animatedVariables: readonly string[];
}

const WDS: EngineComponents = {
  RunSettingsSummary: WdsRunSettingsSummary,
  editorFocusesElements: true,
  settingsEditable: true,
  modelEditable: true,
  criteriaVariables: ["pressure", "velocity", "flow"],
  animatedVariables: ANIMATED_LINK_VARIABLES,
};

const UDS: EngineComponents = {
  RunSettingsSummary: UdsRunSettingsSummary,
  SettingsView: UdsSettingsView,
  EditorView: UdsElementsView,
  AnalysisView: UdsAnalysisView,
  OverviewComposition: UdsOverviewComposition,
  NodeInspectorBody: UdsNodeInspectorBody,
  LinkInspectorBody: UdsLinkInspectorBody,
  RegionInspectorBody: UdsRegionInspectorBody,
  // The drainage Editor reveals a focused element: it shows the element's
  // own kind and scrolls to its row. It could not when it was a single
  // unnavigable table, which is why this was false — being read-only was
  // never the reason, and hiding the affordance for a viewer confused "you
  // cannot change this" with "you cannot find this".
  editorFocusesElements: true,
  settingsEditable: false,
  modelEditable: false,
  // Drainage has no compliance standard in the project criteria file.
  criteriaVariables: [],
  // Conduit flow and velocity are rates the pulse can carry directly.
  // Depth and capacity are states rather than rates — a full pipe is not a
  // fast one — and animating them would have the motion assert something
  // the number does not say.
  animatedVariables: ["flow", "velocity"],
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
