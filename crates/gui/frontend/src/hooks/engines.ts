/**
 * Engine registry (hydra-common spec §2): descriptors for every engine
 * compiled into the backend, fetched once per session via `list_engines`.
 * All engine-identity presentation (label, pill, accent) derives from this
 * registry keyed by each project's `engine` key — never hardcoded.
 *
 * An unresolvable key is an explicit unsupported state (e.g. a project
 * created by a newer Hydra carrying an engine this build lacks):
 * `engineByKey` returns null and surfaces must render an unsupported
 * treatment — never fall back to a default engine.
 */

import { useEffect, useState } from "react";
import { tryInvokeOr } from "./ipc";

/** One source-model file format an engine imports (hydra-common spec §2.2).
 *
 * A file-picker filter, never a validity test — `wds` and `uds` both claim
 * `inp` with incompatible contents, so only the backend parse can decide
 * whether a chosen file is really that engine's model. */
export interface ImportFormat {
  label: string;
  /** Lowercase, no leading dot. */
  extensions: string[];
}

/** Whether this build can actually run the engine (hydra-common spec §2.3).
 * `planned` engines are registered so they can be presented, but no
 * project may be created for one. */
export type EngineStatus = "available" | "planned";

export interface EngineInfo {
  key: string;
  label: string;
  pill: string;
  accent: string;
  summary: string;
  status: EngineStatus;
  import: ImportFormat[];
}

/** Registry served outside a Tauri shell (plain `vite` dev server); mirrors
 * the backend's descriptors. */
export const FALLBACK_ENGINES: EngineInfo[] = [
  {
    key: "wds",
    label: "Water Distribution",
    pill: "WD",
    accent: "#4a90d9",
    summary:
      "Pressurized water distribution network simulation — hydraulics, water quality, and energy on the EPANET data model.",
    status: "available",
    import: [{ label: "EPANET input file", extensions: ["inp"] }],
  },
  {
    key: "uds",
    label: "Urban Drainage",
    pill: "UD",
    accent: "#7a6ff0",
    summary:
      "Stormwater and wastewater collection network simulation — runoff, routing, and water quality on the SWMM data model.",
    status: "planned",
    import: [{ label: "SWMM input file", extensions: ["inp"] }],
  },
  {
    key: "och",
    label: "Open Channel",
    pill: "OC",
    accent: "#3daf75",
    summary:
      "River and open-channel hydraulics — steady and unsteady flow on the HEC-RAS data model.",
    status: "planned",
    import: [
      {
        label: "HEC-RAS project archive",
        extensions: ["zip", "7z", "tar", "gz", "tgz"],
      },
    ],
  },
];

/** Whether `engine` can back a new project in this build. */
export function isEngineAvailable(engine: EngineInfo): boolean {
  return engine.status === "available";
}

/** The engine's accepted extensions as human-facing text, e.g. ".inp" or
 * ".zip, .7z, .tar". Used to tell the user what a drop zone accepts. */
export function importExtensionLabel(engine: EngineInfo): string {
  const exts = engine.import.flatMap((f) => f.extensions);
  return [...new Set(exts)].map((e) => `.${e}`).join(", ");
}

// The registry is static per build — one fetch per session.
let cached: EngineInfo[] | null = null;

export async function getEngines(): Promise<EngineInfo[]> {
  if (cached) return cached;
  const engines = await tryInvokeOr<EngineInfo[]>(
    "list_engines",
    undefined,
    FALLBACK_ENGINES,
  );
  cached = engines.length > 0 ? engines : FALLBACK_ENGINES;
  return cached;
}

/** Descriptor for `key`, or null — an explicit unsupported state. */
export function engineByKey(
  engines: EngineInfo[],
  key: string,
): EngineInfo | null {
  return engines.find((e) => e.key === key) ?? null;
}

/** The engine registry; resolved from the backend once per session. */
export function useEngines(): EngineInfo[] {
  const [engines, setEngines] = useState<EngineInfo[]>(
    () => cached ?? FALLBACK_ENGINES,
  );
  useEffect(() => {
    let cancelled = false;
    void getEngines().then((list) => {
      if (!cancelled) setEngines(list);
    });
    return () => {
      cancelled = true;
    };
  }, []);
  return engines;
}
