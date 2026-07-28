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

export interface EngineInfo {
  key: string;
  label: string;
  pill: string;
  accent: string;
  summary: string;
}

/** Registry served outside a Tauri shell (plain `vite` dev server); mirrors
 * the backend's wds descriptor. */
export const FALLBACK_ENGINES: EngineInfo[] = [
  {
    key: "wds",
    label: "Water Distribution",
    pill: "WD",
    accent: "#4a90d9",
    summary:
      "Pressurized water distribution network simulation — hydraulics, water quality, and energy on the EPANET data model.",
  },
];

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
