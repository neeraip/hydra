import { PROJECT_VIEWS, type ProjectView } from "./projectConfig";

/**
 * Dev-only launch override: land the app on a named project and view.
 *
 * `scripts/screenshot-stage.py` launches `cargo tauri dev` with
 * `VITE_HYDRA_BOOT_PROJECT` / `VITE_HYDRA_BOOT_VIEW` set so marketing
 * screenshots always open on the same page of the same staged network.
 * The override exists only in dev builds: Vite inlines `DEV: false` into
 * a production bundle, so a release binary ignores the variables no
 * matter what the environment says. Precedent for an env-shaped QA hook
 * is `hydra2-updater-mock` (see hooks/useUpdater.ts).
 */
export interface BootEnv {
  DEV: boolean;
  VITE_HYDRA_BOOT_PROJECT?: string;
  VITE_HYDRA_BOOT_VIEW?: string;
}

export interface BootOverride {
  projectId: string;
  /** Validated view, or null to fall back to the project's stored view. */
  view: ProjectView | null;
}

export function bootOverride(env: BootEnv): BootOverride | null {
  if (!env.DEV) return null;
  const projectId = env.VITE_HYDRA_BOOT_PROJECT?.trim();
  if (!projectId) return null;
  const requested = env.VITE_HYDRA_BOOT_VIEW?.trim();
  const view = PROJECT_VIEWS.find((v) => v.id === requested)?.id ?? null;
  return { projectId, view };
}

/**
 * What a launch opens, and whether that counts as the session.
 *
 * These are two questions, and one stored project id used to answer
 * both: "which project is on screen" and "which project should the next
 * launch reopen". A staged screenshot launch is the case that pulls them
 * apart. It opens a project, so the first answer is yes; it must leave
 * no trace, so the second is no.
 *
 * With one value for both, the effect that remembers the open project
 * wrote the staged id into the real session on mount, one render after
 * the code promised it would not, and the effect that checks the
 * restored project still exists read localStorage rather than the
 * project actually open. The second only worked because the first had
 * already run.
 */
export interface LaunchSession {
  /** Project to open, or null to land on Home. */
  projectId: string | null;
  /** View to open it in. Meaningless when `projectId` is null. */
  view: ProjectView;
  /** Whether this launch may be stored as the session to reopen next time. */
  remember: boolean;
}

/**
 * Decide what this launch opens.
 *
 * `storedView` is the per-project view preference, passed in rather than
 * read here so the decision can be tested without a browser.
 */
export function launchSession(
  boot: BootOverride | null,
  storedProjectId: string | null,
  storedView: (projectId: string) => ProjectView | null,
): LaunchSession {
  if (boot) {
    return {
      projectId: boot.projectId,
      view: boot.view ?? storedView(boot.projectId) ?? "canvas",
      remember: false,
    };
  }
  return {
    projectId: storedProjectId,
    view: (storedProjectId && storedView(storedProjectId)) || "canvas",
    remember: true,
  };
}
