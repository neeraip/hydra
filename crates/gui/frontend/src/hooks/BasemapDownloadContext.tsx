/**
 * BasemapDownloadContext — app-wide owner of the offline-basemap download
 * lifecycle.
 *
 * Downloads run on a backend worker thread and outlive any modal, so the
 * single `basemap:download` subscription lives here: progress is exposed to
 * whichever UI is open (download modal, settings section, coverage chip) and
 * the completion / failure / cancellation toast fires even when everything
 * else has been closed. `storeGeneration` bumps whenever the region store
 * changes so listings can refetch.
 */

import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { useAppState } from "../AppContext";
import {
  type BasemapBbox,
  cancelBasemapDownload,
  downloadBasemapRegion,
  formatBytes,
  listenBasemapDownload,
} from "./basemaps";

/** Live progress of the in-flight download. */
export interface ActiveBasemapDownload {
  regionName: string;
  phase: "planning" | "downloading";
  doneBytes: number;
  totalBytes: number;
}

interface BasemapDownloadContextValue {
  /** Current in-flight download, or null when idle. */
  active: ActiveBasemapDownload | null;
  /** Bumped whenever the region store changed (download completed, region
   *  deleted via `bumpStore`) — listings refetch on it. */
  storeGeneration: number;
  /** Start a background region download (throws on backend rejection, e.g.
   *  when another download is already running). */
  startDownload: (args: {
    name: string;
    bbox: BasemapBbox;
    projectId?: string | null;
  }) => Promise<void>;
  /** Request cooperative cancellation of the in-flight download. */
  cancelDownload: () => void;
  /** Notify listeners that the region store changed (e.g. after a delete). */
  bumpStore: () => void;
}

const Ctx = createContext<BasemapDownloadContextValue | null>(null);

export function BasemapDownloadProvider({ children }: { children: ReactNode }) {
  const { showToast } = useAppState();
  const [active, setActive] = useState<ActiveBasemapDownload | null>(null);
  const [storeGeneration, setStoreGeneration] = useState(0);

  const bumpStore = useCallback(() => setStoreGeneration((g) => g + 1), []);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    listenBasemapDownload((ev) => {
      switch (ev.phase) {
        case "planning":
        case "downloading":
          setActive({
            regionName: ev.regionName,
            phase: ev.phase,
            doneBytes: ev.doneBytes,
            totalBytes: ev.totalBytes,
          });
          break;
        case "complete":
          setActive(null);
          setStoreGeneration((g) => g + 1);
          showToast(
            `Offline basemap "${ev.regionName}" downloaded · ${formatBytes(ev.totalBytes)}`,
            "success",
          );
          break;
        case "cancelled":
          setActive(null);
          showToast(`Download of "${ev.regionName}" cancelled`);
          break;
        case "failed":
          setActive(null);
          showToast(
            `Offline basemap download failed: ${ev.error ?? "unknown error"}`,
            "error",
          );
          break;
      }
    })
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch((e) => {
        // Expected outside a Tauri shell (plain vite dev server).
        // eslint-disable-next-line no-console
        console.warn("[basemap-download] failed to register listener:", e);
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [showToast]);

  const startDownload = useCallback(
    async (args: {
      name: string;
      bbox: BasemapBbox;
      projectId?: string | null;
    }) => {
      // Optimistic: the backend's first "planning" event can lag the archive
      // header fetch, and UI (coverage chip, settings buttons) should react
      // immediately.
      setActive({
        regionName: args.name,
        phase: "planning",
        doneBytes: 0,
        totalBytes: 0,
      });
      try {
        await downloadBasemapRegion(args);
      } catch (err) {
        setActive(null);
        throw err;
      }
    },
    [],
  );

  const cancelDownload = useCallback(() => {
    void cancelBasemapDownload();
  }, []);

  const value = useMemo(
    () => ({
      active,
      storeGeneration,
      startDownload,
      cancelDownload,
      bumpStore,
    }),
    [active, storeGeneration, startDownload, cancelDownload, bumpStore],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useBasemapDownload(): BasemapDownloadContextValue {
  const ctx = useContext(Ctx);
  if (!ctx)
    throw new Error(
      "useBasemapDownload must be used within BasemapDownloadProvider",
    );
  return ctx;
}
