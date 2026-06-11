export type ScenarioState = "not-run" | "running" | "simulated" | "calibrated";

export interface Scenario {
  id: string;
  name: string;
  state: ScenarioState;
  parentId: string | null;
  children: Scenario[];
}
