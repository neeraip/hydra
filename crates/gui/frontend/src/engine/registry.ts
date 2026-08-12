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
import {
  CatalogCriteriaControl,
  WdsCriteriaControl,
} from "../components/panels/CriteriaControl";
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
  /** The project toolbar's criteria control: a chip opening this engine's
   * criteria editor. Criteria are project-scoped, so the control is too —
   * it reaches the canvas it recolours, the results it judges, and the
   * report it is exported into. Absent = the wds editor. */
  CriteriaControl?: ComponentType;
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
  /**
   * What this GUI can do to the engine's model.
   *
   * Two capabilities rather than one flag, because they are two
   * questions and the engines answer them differently. Moving an
   * element and creating one need different things of an engine: the
   * first needs somewhere to put a position, the second needs defaults
   * for every field a new element has. Drainage has the first and not
   * yet the second.
   *
   * They were one flag while drainage could do neither, which is when
   * a single value answering two questions looks correct. Editing
   * affordances are hidden rather than offered-and-refused, so a flag
   * that over-claims shows up as a gesture that does nothing.
   */
  editing: {
    /** Positions can be changed: the edit tool, dragging on the canvas. */
    geometry: boolean;
    /** An element's identifier can be changed. Its own capability
     * because renaming maintains references, where creating supplies
     * defaults — an engine can do the first without the second. */
    rename: boolean;
    /** Elements can be created and deleted: the add tools, create
     * modals, and the editor's row actions. A project can only begin
     * from an imported model without this. */
    structure: boolean;
    /** The model's title can be rewritten. Its own capability because
     * it is its own mutation — a model whose elements are fixed can
     * still be described, and one whose title is fixed can still be
     * rearranged. */
    title: boolean;
  };
  /**
   * Result-variable ids whose motion the canvas can animate, per element
   * class.
   *
   * Motion is about the water, so what it can animate depends on what an
   * engine publishes and on what that variable means. Mostly that is a
   * **rate** — how fast and which way. It is not only rates: water
   * distribution animates Status, whose pulse says whether anything is
   * moving at all rather than how fast, and Quality, a concentration the
   * water carries at its own speed. What never animates is a reading that
   * stands still while the water does not — a conduit's capacity and a
   * node's depth are states (a full pipe is not a fast one), and animating
   * them would have the motion assert something the number does not say.
   *
   * Keyed by class because the two animate differently and qualify
   * separately: links pulse along their length, points ring outward from
   * their centre. A single flat list served the links and silently
   * answered for everything else.
   *
   * Matching by id alone would not do either: `flow` and `velocity` happen
   * to be spelled the same in both engines today, and the sentence offered
   * to a reader whose selection is not animated was the water distribution
   * one for everybody — a drainage map named Unit headloss and Quality,
   * which drainage does not have.
   *
   * Empty lists mean the engine animates nothing of that class, and the
   * toggle stays inert wherever it appears.
   */
  animatedVariables: {
    /** Point (node) variables — rates crossing the node's boundary. */
    readonly point: readonly string[];
    /** Polyline (link) variables — rates along the conveyance. */
    readonly polyline: readonly string[];
  };
}

const WDS: EngineComponents = {
  RunSettingsSummary: WdsRunSettingsSummary,
  CriteriaControl: WdsCriteriaControl,
  editorFocusesElements: true,
  settingsEditable: true,
  editing: { geometry: true, rename: true, structure: true, title: true },
  animatedVariables: {
    // Demand is a rate and would ring honestly, but it is nonzero at
    // nearly every junction — unlike drainage flooding, whose sparsity is
    // what makes the rings cheap and worth reading. Measure before
    // turning it on.
    point: [],
    polyline: ANIMATED_LINK_VARIABLES,
  },
};

const UDS: EngineComponents = {
  RunSettingsSummary: UdsRunSettingsSummary,
  SettingsView: UdsSettingsView,
  EditorView: UdsElementsView,
  AnalysisView: UdsAnalysisView,
  CriteriaControl: CatalogCriteriaControl,
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
  // Positions and names are editable: a drainage node's coordinate is a
  // line in a preserved display section and its name appears in the
  // control-rule text, and the backend maintains both. Structure is not
  // — creating an element needs a default for every field its kind
  // carries, and nothing supplies those yet.
  editing: { geometry: true, rename: true, structure: false, title: false },
  // Conduit flow and velocity are rates the pulse can carry directly.
  // Depth and capacity are states rather than rates — a full pipe is not a
  // fast one — and animating them would have the motion assert something
  // the number does not say.
  //
  // Flooding is the node counterpart: a rate leaving the network at the
  // surface, which a ring expanding from the node is the picture of. It is
  // also zero nearly everywhere, so the colour ramp over it is almost
  // uniform and the handful of nodes that matter are hard to find — which
  // is the case for motion, and what keeps the moving set small. The
  // inflows are rates too and would work identically; they are nonzero
  // nearly everywhere, so they wait on measurement.
  animatedVariables: { point: ["flooding"], polyline: ["flow", "velocity"] },
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
