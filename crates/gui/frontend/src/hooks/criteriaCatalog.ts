/** The active engine's criteria catalog (hydra-common §7.2): what its
 * standard is made of, as data.
 *
 * Cached per project and fetched once however many readers ask — the
 * catalog is static per engine, so a second fetch could only ever return
 * the same answer. */

import { useEffect, useState } from "react";
import type { Criterion } from "../components/analysis/criteria";
import { tryInvokeOr } from "./ipc";

const cache = new Map<string, Criterion[]>();
const EMPTY: Criterion[] = [];

export function useCriteriaCatalog(projectId: string | null): Criterion[] {
  const [catalog, setCatalog] = useState<Criterion[]>(
    () => (projectId ? cache.get(projectId) : undefined) ?? EMPTY,
  );

  useEffect(() => {
    if (!projectId) {
      setCatalog(EMPTY);
      return;
    }
    const cached = cache.get(projectId);
    if (cached) {
      setCatalog(cached);
      return;
    }
    let cancelled = false;
    void tryInvokeOr<Criterion[]>(
      "get_criteria_catalog",
      { projectId },
      EMPTY,
    ).then((read) => {
      if (cancelled) return;
      if (read.length > 0) cache.set(projectId, read);
      setCatalog(read);
    });
    return () => {
      cancelled = true;
    };
  }, [projectId]);

  return catalog;
}
