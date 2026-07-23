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
    | "theme-dark"
    | "theme-light"
    | "theme-system"
    | "compare";
  projectId?: string;
}
