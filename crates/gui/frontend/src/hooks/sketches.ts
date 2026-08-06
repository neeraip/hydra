/**
 * Network outlines for the home page's cards.
 *
 * Fetched per project rather than with the projects list, because the
 * projects table never draws one and would carry every drawing it loaded.
 * A project with no sketch is ordinary: it has not been opened since the
 * feature landed, or it carries no coordinates.
 */

import { useEffect, useState } from "react";
import type { Sketch } from "../components/ui/NetworkSketch";
import { tryInvokeOr } from "./ipc";

/** Sketches for the given projects, keyed by id. Absent while loading. */
export function useSketches(projectIds: string[]): Map<string, Sketch> {
  const [sketches, setSketches] = useState<Map<string, Sketch>>(new Map());
  // Joined so the effect re-runs when the set changes, not when the array
  // identity does — the caller rebuilds it on every render.
  const key = projectIds.join(",");

  useEffect(() => {
    let cancelled = false;
    const ids = key ? key.split(",") : [];
    Promise.all(
      ids.map(async (id) => {
        const s = await tryInvokeOr<Sketch | null>(
          "get_project_sketch",
          { projectId: id },
          null,
        );
        return [id, s] as const;
      }),
    ).then((pairs) => {
      if (cancelled) return;
      const next = new Map<string, Sketch>();
      for (const [id, s] of pairs) if (s) next.set(id, s);
      setSketches(next);
    });
    return () => {
      cancelled = true;
    };
  }, [key]);

  return sketches;
}
