import { useEffect, useState } from "react";
import { useAppState } from "../AppContext";
import { Toggle } from "../components/ui/Toggle";
import { getVersions, type Versions } from "../hooks";
import { useUpdater } from "../hooks/useUpdater";
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

/** Section heading. A real `<h2>` rather than a styled div: the page has one
 * `<h1>` and seven visually obvious groups, none of which existed for a
 * screen reader navigating by heading. */
function Section({ children }: { children: React.ReactNode }) {
  return (
    <h2
      style={{
        // The element's own defaults would fight the type scale below.
        margin: 0,
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
    </h2>
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
              color: "var(--text-secondary)",
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

/** "Software updates" row: manual update check with inline status. Hidden
 * entirely when this install can't self-update (dev builds, Linux deb/rpm —
 * those update via the package manager). The row's description doubles as
 * the status line; the single button carries the whole flow. */
function UpdatesRow() {
  const { updater, supported, install, restart, checkNow } = useUpdater();
  if (supported !== true) return null;

  const description =
    updater.phase === "checking"
      ? "Checking for updates…"
      : updater.phase === "upToDate"
        ? "You're up to date."
        : updater.phase === "checkFailed"
          ? `Couldn't check for updates: ${updater.message}`
          : updater.phase === "available"
            ? `Version ${updater.version} is available.`
            : updater.phase === "downloading"
              ? `Downloading version ${updater.version}…`
              : updater.phase === "ready"
                ? `Version ${updater.version} is ready to install.`
                : updater.phase === "error"
                  ? `Update failed: ${updater.message}`
                  : "Check if a newer version of Hydra is available.";

  const busy = updater.phase === "checking" || updater.phase === "downloading";
  const label =
    updater.phase === "checking"
      ? "Checking…"
      : updater.phase === "available"
        ? `Download v${updater.version}`
        : updater.phase === "downloading"
          ? updater.percent !== null
            ? `Downloading… ${updater.percent}%`
            : "Downloading…"
          : updater.phase === "ready"
            ? "Restart to update"
            : updater.phase === "error"
              ? "Retry download"
              : "Check for updates";
  const onClick =
    updater.phase === "available" || updater.phase === "error"
      ? install
      : updater.phase === "ready"
        ? restart
        : checkNow;

  return (
    <SettingRow label="Software updates" description={description}>
      <button
        type="button"
        disabled={busy}
        onClick={onClick}
        style={{
          padding: "5px 14px",
          border: "1px solid var(--border-hover)",
          borderRadius: 6,
          background: "transparent",
          color: busy ? "var(--text-tertiary)" : "var(--text-primary)",
          cursor: busy ? "default" : "pointer",
          fontSize: 13,
          fontFamily: "var(--font-ui)",
          fontVariantNumeric: "tabular-nums",
        }}
      >
        {label}
      </button>
    </SettingRow>
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

export function SettingsPage() {
  const { openBasemapProvidersModal } = useAppState();
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
          // Centred rather than left-aligned: the column is deliberately
          // narrow so each control stays beside its label, but pinning it left
          // pushed all the slack to one side, which read as a layout fault
          // rather than a margin. Matches the report preview's constrained
          // page.
          margin: "0 auto",
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
        {/* Appearance — theme, units and basemap are all "how things are
            shown", and each had its own header for a single row. */}
        <Section>Appearance</Section>
        <SettingRow label="Theme" description="Choose dark or light mode.">
          <ThemeToggle />
        </SettingRow>
        <SettingRow
          label="Display units"
          description="How values are shown and entered throughout the app. Files and exports (INP, CSV, GeoJSON) always remain in the model's native/SI units."
        >
          <UnitSystemToggle />
        </SettingRow>
        <SettingRow
          label="Basemap providers"
          description="Connect imagery providers (Mapbox, MapTiler, Esri) and choose which basemap styles appear in the canvas picker."
        >
          <button
            type="button"
            onClick={openBasemapProvidersModal}
            style={{
              padding: "5px 14px",
              border: "1px solid var(--border-hover)",
              borderRadius: 6,
              background: "transparent",
              color: "var(--text-primary)",
              cursor: "pointer",
              fontSize: 13,
              fontFamily: "var(--font-ui)",
            }}
          >
            Manage providers…
          </button>
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
        {/* About */}
        <Section>About</Section>
        <UpdatesRow />
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
      </div>
    </div>
  );
}
