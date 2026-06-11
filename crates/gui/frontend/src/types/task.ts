export type TaskStatus = "queued" | "completed" | "running" | "failed";

export interface TaskHistoryEntry {
  at: number; // Date.now() ms
  label: string;
}

export interface Task {
  id: string;
  projectId?: string;
  /** null = base model; UUID = scenario */
  scenarioId?: string | null;
  projectName: string;
  scenarioName: string;
  status: TaskStatus;
  timeLabel: string;
  // ── live progress (populated from simulation_progress events) ──
  phase?: "hydraulics" | "quality";
  progressPercent?: number;
  /** Raw message from the backend (e.g. "Hydraulics: step 3/12"). */
  progressMessage?: string;
  /** Simulated model-time elapsed, in seconds (current phase). */
  simulatedSeconds?: number;
  /** Total simulation duration, in seconds. */
  durationSeconds?: number;
  /** Per-phase progress: hydraulics 0–100. */
  hydraulicsPercent?: number;
  hydraulicsDone?: boolean;
  /** Per-phase progress: quality 0–100. Only meaningful when hasQuality. */
  qualityPercent?: number;
  qualityDone?: boolean;
  /** True when quality is enabled for this simulation run. */
  hasQuality?: boolean;
  /** Compact timestamped log shown in the tray. */
  history?: TaskHistoryEntry[];
  // ── non-progress fields ──
  errorMessage?: string;
  primaryAction?: string;
  secondaryAction?: string;
}
