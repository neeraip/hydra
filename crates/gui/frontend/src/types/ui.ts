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
    | "compare";
  projectId?: string;
}

export interface SectionGroup {
  label: string;
  sections: { id: string; label: string; count: number }[];
}
