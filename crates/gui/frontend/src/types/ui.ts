export type CommandCategory = "Recent" | "Navigate" | "Simulate" | "Actions";

export interface Command {
  id: string;
  label: string;
  description?: string;
  category: CommandCategory;
  shortcut?: string;
  action?:
    | "open-project"
    | "nav-canvas"
    | "nav-overview"
    | "nav-report"
    | "nav-scenarios"
    | "nav-analysis"
    | "nav-editor"
    | "nav-settings"
    | "nav-home"
    | "nav-projects"
    | "run-sim"
    | "canvas-layout-toggle"
    | "canvas-layout-map"
    | "canvas-layout-schematic"
    | "canvas-tool-select"
    | "canvas-tool-edit"
    | "canvas-tool-add-node"
    | "canvas-tool-add-link"
    | "canvas-tool-measure"
    | "canvas-zoom-in"
    | "canvas-zoom-out"
    | "canvas-fit-network"
    | "canvas-reset-north"
    | "canvas-toggle-view"
    | "undo"
    | "redo"
    | "save-changes"
    | "shortcut-card"
    | "units-default-source"
    | "units-default-si"
    | "units-default-us"
    | "units-project-source"
    | "units-project-si"
    | "units-project-us"
    | "units-project-inherit"
    | "theme-dark"
    | "theme-light"
    | "theme-system"
    | "compare"
    | "switch-scenario";
  projectId?: string;
}

/**
 * Display-only category union — extends the data-layer `CommandCategory`
 * with the synthetic "Page" and "Scenarios" groups the palette injects from
 * the user's current view. The data layer doesn't know about those.
 */
export type DisplayCategory = CommandCategory | "Page" | "Scenarios";

/** A palette entry, which may be built from live state rather than declared. */
export interface DynamicCommand extends Omit<Command, "category"> {
  projectId?: string;
  /** Target for the "switch-scenario" action. */
  scenarioId?: string;
  category: DisplayCategory;
}
