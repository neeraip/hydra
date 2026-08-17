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
