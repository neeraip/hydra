import { useEffect, useState } from "react";
import { useAppState } from "../AppContext";
import { BasemapDownloadModal } from "../components/modals/BasemapDownloadModal";
import { DeleteConfirmModal } from "../components/modals/DeleteConfirmModal";
import { Toggle } from "../components/ui/Toggle";
import { getVersions, reconcileProjects, type Versions } from "../hooks";
import { useBasemapDownload } from "../hooks/BasemapDownloadContext";
import {
  type BasemapRegionInfo,
  type BasemapStorage,
  deleteBasemapRegion,
  formatBytes,
  listBasemapRegions,
  regionSizeLabel,
} from "../hooks/basemaps";
import { formatIpcError } from "../hooks/ipc";
import { setUnitSystem, type UnitSystem, useUnitSystem } from "../units";

const SK = {
  reducedMotion: "hydra2-reduced-motion",
  highContrast: "hydra2-high-contrast",
  // Must match AppContext's STORAGE_RESTORE_SESSION.
  restoreSession: "hydra2-restore-session",
} as const;

function getBool(key: string, fallback: boolean): boolean {
  const v = localStorage.getItem(key);
  return v === null ? fallback : v === "true";
}

function Section({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        marginTop: 32,
        marginBottom: 2,
        fontSize: 11,
        fontWeight: 600,
        letterSpacing: "0.08em",
        textTransform: "uppercase",
        color: "var(--text-tertiary)",
      }}
    >
      {children}
    </div>
  );
}

function SettingRow({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "12px 0",
        borderBottom: "1px solid var(--border)",
        gap: 24,
      }}
    >
      <div>
        <div
          style={{
            fontSize: 13,
            color: "var(--text-primary)",
            fontWeight: 500,
          }}
        >
          {label}
        </div>
        {description && (
          <div
            style={{
              fontSize: 12,
              color: "var(--text-tertiary)",
              marginTop: 2,
              lineHeight: 1.5,
            }}
          >
            {description}
          </div>
        )}
      </div>
      <div style={{ flexShrink: 0 }}>{children}</div>
    </div>
  );
}

function ThemeToggle() {
  const { theme, setTheme } = useAppState();
  return (
    <div style={{ display: "flex", gap: 6 }}>
      {(["dark", "light", "system"] as const).map((t) => (
        <button
          type="button"
          key={t}
          onClick={() => setTheme(t)}
          style={{
            padding: "5px 14px",
            border: "1px solid",
            borderColor: theme === t ? "var(--accent)" : "var(--border-hover)",
            borderRadius: 6,
            background: theme === t ? "var(--accent-dim)" : "transparent",
            color: theme === t ? "var(--accent)" : "var(--text-secondary)",
            cursor: "pointer",
            fontSize: 13,
            fontFamily: "var(--font-ui)",
            fontWeight: theme === t ? 500 : 400,
            transition: "all var(--t-fast)",
            textTransform: "capitalize",
          }}
        >
          {t}
        </button>
      ))}
    </div>
  );
}

const UNIT_OPTIONS: Array<{ value: UnitSystem; label: string }> = [
  { value: "si", label: "SI (metric)" },
  { value: "us", label: "US customary" },
];

function UnitSystemToggle() {
  const unitSystem = useUnitSystem();
  return (
    <div style={{ display: "flex", gap: 6 }}>
      {UNIT_OPTIONS.map((o) => (
        <button
          type="button"
          key={o.value}
          onClick={() => setUnitSystem(o.value)}
          style={{
            padding: "5px 14px",
            border: "1px solid",
            borderColor:
              unitSystem === o.value ? "var(--accent)" : "var(--border-hover)",
            borderRadius: 6,
            background:
              unitSystem === o.value ? "var(--accent-dim)" : "transparent",
            color:
              unitSystem === o.value
                ? "var(--accent)"
                : "var(--text-secondary)",
            cursor: "pointer",
            fontSize: 13,
            fontFamily: "var(--font-ui)",
            fontWeight: unitSystem === o.value ? 500 : 400,
            transition: "all var(--t-fast)",
          }}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

// Shared style for the small outline action buttons in this page.
const actionButtonStyle = (disabled: boolean): React.CSSProperties => ({
  padding: "5px 14px",
  border: "1px solid var(--border-hover)",
  borderRadius: 6,
  background: "transparent",
  color: "var(--text-primary)",
  cursor: disabled ? "not-allowed" : "pointer",
  fontSize: 13,
  fontFamily: "var(--font-ui)",
  opacity: disabled ? 0.5 : 1,
  transition: "opacity var(--t-fast)",
});

/** Small pill used for the per-region project count / "Unused" badges. */
function RegionBadge({
  children,
  tone = "neutral",
}: {
  children: React.ReactNode;
  tone?: "neutral" | "warn";
}) {
  return (
    <span
      style={{
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: "0.04em",
        textTransform: "uppercase",
        padding: "2px 7px",
        borderRadius: 999,
        whiteSpace: "nowrap",
        color:
          tone === "warn" ? "var(--status-warning)" : "var(--text-secondary)",
        background:
          tone === "warn" ? "rgba(220,160,60,0.12)" : "var(--bg-input)",
        border: "1px solid var(--border)",
      }}
    >
      {children}
    </span>
  );
}

function RegionRow({
  region,
  unused,
  onDelete,
}: {
  region: BasemapRegionInfo;
  unused: boolean;
  onDelete: () => void;
}) {
  const created = new Date(region.createdAt * 1000).toLocaleDateString();
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        padding: "12px 0",
        borderBottom: "1px solid var(--border)",
        gap: 24,
      }}
    >
      <div style={{ minWidth: 0 }}>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            minWidth: 0,
          }}
        >
          <span
            style={{
              fontSize: 13,
              color: "var(--text-primary)",
              fontWeight: 500,
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
            }}
          >
            {region.name}
          </span>
          <RegionBadge>
            {region.projectIds.length} project
            {region.projectIds.length === 1 ? "" : "s"}
          </RegionBadge>
          {unused && <RegionBadge tone="warn">Unused</RegionBadge>}
        </div>
        <div
          style={{
            fontSize: 12,
            color: "var(--text-tertiary)",
            marginTop: 2,
            lineHeight: 1.5,
          }}
        >
          Created {created} · {regionSizeLabel(region)} ·{" "}
          {region.tileCount.toLocaleString()} tiles
        </div>
      </div>
      <button
        type="button"
        onClick={onDelete}
        style={{ ...actionButtonStyle(false), flexShrink: 0 }}
        onMouseEnter={(e) => {
          (e.currentTarget as HTMLButtonElement).style.color =
            "var(--status-error)";
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLButtonElement).style.color =
            "var(--text-primary)";
        }}
      >
        Delete
      </button>
    </div>
  );
}

function OfflineBasemapsSection() {
  const { showToast } = useAppState();
  const { active, storeGeneration, bumpStore } = useBasemapDownload();
  const [storage, setStorage] = useState<BasemapStorage | null>(null);
  const [loaded, setLoaded] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<BasemapRegionInfo | null>(
    null,
  );
  const [removingUnused, setRemovingUnused] = useState(false);
  const [downloadOpen, setDownloadOpen] = useState(false);

  // `storeGeneration` bumps when a download completes or a region is
  // deleted, so the listing stays live while this page is open.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `storeGeneration` is an intentional refetch trigger.
  useEffect(() => {
    let cancelled = false;
    listBasemapRegions().then((s) => {
      if (!cancelled) {
        setStorage(s);
        setLoaded(true);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [storeGeneration]);

  const regions = storage?.regions ?? [];
  const unusedIds = new Set(storage?.unusedRegionIds ?? []);
  const downloadBusy = active !== null;

  const handleConfirmDelete = async () => {
    if (!pendingDelete) return;
    const { id, name } = pendingDelete;
    setPendingDelete(null);
    try {
      const { freedBytes } = await deleteBasemapRegion(id);
      showToast(
        `Removed "${name}" · ${formatBytes(freedBytes)} freed`,
        "success",
      );
    } catch (err) {
      showToast(`Failed to delete region: ${formatIpcError(err)}`, "error");
    }
    bumpStore();
  };

  const handleRemoveUnused = async () => {
    if (!storage || storage.unusedRegionIds.length === 0) return;
    setRemovingUnused(true);
    let freed = 0;
    let removed = 0;
    try {
      // Sequential on purpose — the store serialises writes anyway and this
      // keeps per-region failures attributable.
      for (const id of storage.unusedRegionIds) {
        const { freedBytes } = await deleteBasemapRegion(id);
        freed += freedBytes;
        removed += 1;
      }
      showToast(
        `Removed ${removed} unused region${removed === 1 ? "" : "s"} · ${formatBytes(freed)} freed`,
        "success",
      );
    } catch (err) {
      showToast(
        `Failed to remove unused regions: ${formatIpcError(err)}`,
        "error",
      );
    } finally {
      setRemovingUnused(false);
      bumpStore();
    }
  };

  return (
    <>
      <Section>Offline basemaps</Section>
      {/* Header: store totals + actions */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "12px 0",
          borderBottom: "1px solid var(--border)",
          gap: 24,
        }}
      >
        <div>
          <div
            style={{
              fontSize: 13,
              color: "var(--text-primary)",
              fontWeight: 500,
            }}
          >
            {storage
              ? `${regions.length} region${regions.length === 1 ? "" : "s"} · ${formatBytes(storage.diskBytes)} on disk`
              : loaded
                ? "Offline basemap store unavailable"
                : "Loading…"}
          </div>
          <div
            style={{
              fontSize: 12,
              color: "var(--text-tertiary)",
              marginTop: 2,
              lineHeight: 1.5,
            }}
          >
            Map areas downloaded for use without an internet connection. Regions
            share tiles, so deleting one only frees its unique data.
          </div>
        </div>
        <div style={{ display: "flex", gap: 6, flexShrink: 0 }}>
          {storage !== null && storage.unusedRegionIds.length > 0 && (
            <button
              type="button"
              disabled={removingUnused}
              onClick={handleRemoveUnused}
              style={actionButtonStyle(removingUnused)}
            >
              {removingUnused
                ? "Removing…"
                : `Remove unused (${storage.unusedRegionIds.length})`}
            </button>
          )}
          <button
            type="button"
            disabled={downloadBusy}
            onClick={() => setDownloadOpen(true)}
            style={actionButtonStyle(downloadBusy)}
            title={
              downloadBusy ? "A basemap download is already running" : undefined
            }
          >
            {downloadBusy
              ? `Downloading ${active.regionName}…`
              : "Download region…"}
          </button>
        </div>
      </div>
      {/* Region rows */}
      {regions.map((r) => (
        <RegionRow
          key={r.id}
          region={r}
          unused={unusedIds.has(r.id)}
          onDelete={() => setPendingDelete(r)}
        />
      ))}
      {storage !== null && regions.length === 0 && (
        <div
          style={{
            padding: "12px 0",
            fontSize: 12,
            color: "var(--text-tertiary)",
          }}
        >
          No offline regions downloaded yet.
        </div>
      )}
      <DeleteConfirmModal
        open={pendingDelete !== null}
        elementKind="region"
        elementId={pendingDelete?.name ?? ""}
        title="Delete offline region"
        message={
          pendingDelete && (
            <>
              Delete{" "}
              <strong style={{ color: "var(--text-primary)" }}>
                {pendingDelete.name}
              </strong>
              ? Tiles not shared with other regions (
              {formatBytes(pendingDelete.uniqueBytes)}) will be freed.
            </>
          )
        }
        onConfirm={handleConfirmDelete}
        onCancel={() => setPendingDelete(null)}
      />
      <BasemapDownloadModal
        open={downloadOpen}
        initialBbox={null}
        initialName="Region"
        onClose={() => setDownloadOpen(false)}
      />
    </>
  );
}

export function SettingsPage() {
  const { showToast } = useAppState();
  const [reducedMotion, setReducedMotionRaw] = useState(() =>
    getBool(SK.reducedMotion, false),
  );
  const [highContrast, setHighContrastRaw] = useState(() =>
    getBool(SK.highContrast, false),
  );
  const [restoreSession, setRestoreSessionRaw] = useState(() =>
    getBool(SK.restoreSession, true),
  );
  const [versions, setVersions] = useState<Versions | null>(null);
  const [isReconciling, setIsReconciling] = useState(false);

  // Wrap setters to also persist to localStorage.
  const setRestoreSession = (v: boolean) => {
    setRestoreSessionRaw(v);
    localStorage.setItem(SK.restoreSession, String(v));
  };
  const setReducedMotion = (v: boolean) => {
    setReducedMotionRaw(v);
    localStorage.setItem(SK.reducedMotion, String(v));
    document.documentElement.setAttribute("data-reduced-motion", String(v));
  };
  const setHighContrast = (v: boolean) => {
    setHighContrastRaw(v);
    localStorage.setItem(SK.highContrast, String(v));
    document.documentElement.setAttribute("data-high-contrast", String(v));
  };

  useEffect(() => {
    getVersions()
      .then(setVersions)
      .catch((err) => {
        // Leave `versions` null — the UI falls back to "—".
        console.error("Failed to load version info:", err);
      });
  }, []);

  return (
    <div
      style={{
        flex: 1,
        height: "100%",
        overflow: "auto",
        animation: "fadeIn 180ms ease-out",
      }}
    >
      <div
        style={{
          maxWidth: 680,
          padding: "40px 48px",
        }}
      >
        <h1
          style={{
            margin: "0 0 4px",
            fontSize: 22,
            fontWeight: 700,
            letterSpacing: "-0.015em",
          }}
        >
          Settings
        </h1>
        <p
          style={{
            margin: "0 0 4px",
            color: "var(--text-secondary)",
            fontSize: 14,
          }}
        >
          Appearance, accessibility, and maintenance tools.
        </p>
        {/* General */}
        <Section>General</Section>
        <SettingRow
          label="Reopen last project on launch"
          description="Start Hydra straight back in the project you last had open."
        >
          <Toggle checked={restoreSession} onChange={setRestoreSession} />
        </SettingRow>
        {/* Appearance */}
        <Section>Appearance</Section>
        <SettingRow label="Theme" description="Choose dark or light mode.">
          <ThemeToggle />
        </SettingRow>
        {/* Units */}
        <Section>Units</Section>
        <SettingRow
          label="Display units"
          description="How values are shown and entered throughout the app. Files and exports (INP, CSV, GeoJSON) always remain in the model's native/SI units."
        >
          <UnitSystemToggle />
        </SettingRow>
        {/* Accessibility */}
        <Section>Accessibility</Section>
        <SettingRow
          label="Reduce motion"
          description="Suppress non-essential animations such as panel slides and pump-flow tickers."
        >
          <Toggle checked={reducedMotion} onChange={setReducedMotion} />
        </SettingRow>
        <SettingRow
          label="High-contrast mode"
          description="Increase contrast for borders, focus rings, and status colours."
        >
          <Toggle checked={highContrast} onChange={setHighContrast} />
        </SettingRow>
        {/* Offline basemaps */}
        <OfflineBasemapsSection />
        {/* About */}
        <Section>About</Section>
        <div
          style={{
            padding: "12px 0",
            display: "flex",
            flexDirection: "column",
            gap: 8,
          }}
        >
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              fontSize: 13,
            }}
          >
            <span style={{ color: "var(--text-secondary)" }}>Application</span>
            <span
              style={{
                color: "var(--text-primary)",
                fontVariantNumeric: "tabular-nums",
              }}
            >
              v{versions?.app ?? "—"}
            </span>
          </div>
          <div
            style={{
              display: "flex",
              justifyContent: "space-between",
              fontSize: 13,
            }}
          >
            <span style={{ color: "var(--text-secondary)" }}>Hydra engine</span>
            <span
              style={{
                color: "var(--text-primary)",
                fontVariantNumeric: "tabular-nums",
              }}
            >
              v{versions?.hydra ?? "—"}
            </span>
          </div>
        </div>
        {/* Advanced */}
        <Section>Advanced</Section>
        <SettingRow
          label="Repair project library"
          description="Scan the projects folder for orphaned bundles and re-import them. Also flags projects whose folder is missing."
        >
          <button
            type="button"
            disabled={isReconciling}
            onClick={async () => {
              setIsReconciling(true);
              try {
                const report = await reconcileProjects();
                const parts: string[] = [];
                if (report.recovered > 0)
                  parts.push(
                    `Recovered ${report.recovered} project${report.recovered === 1 ? "" : "s"}`,
                  );
                if (report.folderMissing.length > 0)
                  parts.push(
                    `${report.folderMissing.length} folder${report.folderMissing.length === 1 ? "" : "s"} missing`,
                  );
                showToast(
                  parts.length > 0 ? parts.join(" \u00b7 ") : "No issues found",
                );
              } finally {
                setIsReconciling(false);
              }
            }}
            style={{
              padding: "5px 14px",
              border: "1px solid var(--border-hover)",
              borderRadius: 6,
              background: "transparent",
              color: "var(--text-primary)",
              cursor: isReconciling ? "not-allowed" : "pointer",
              fontSize: 13,
              fontFamily: "var(--font-ui)",
              opacity: isReconciling ? 0.5 : 1,
              transition: "opacity var(--t-fast)",
            }}
          >
            {isReconciling ? "Scanning\u2026" : "Repair now"}
          </button>
        </SettingRow>
      </div>
    </div>
  );
}
