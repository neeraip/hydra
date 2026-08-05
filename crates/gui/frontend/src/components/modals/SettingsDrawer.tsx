import { XMarkIcon } from "@heroicons/react/16/solid";
import { useEffect, useState } from "react";
import { useAppState } from "../../AppContext";
import { getVersions, openDataFolder, type Versions } from "../../hooks";
import { useUpdater } from "../../hooks/useUpdater";
import { startPerfSpan } from "../../perfTrace";
import {
  parseTextScale,
  readTextScale,
  setTextScale,
  TEXT_SCALES,
} from "../../textScale";
import {
  setUnitPreference,
  type UnitPreference,
  useUnitPreference,
} from "../../units";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";
import { Toggle } from "../ui/Toggle";

/** Matches the `inputStyle` the editors use for their selects, so a
 * dropdown looks the same wherever it appears. */
const SETTINGS_SELECT: React.CSSProperties = {
  height: 28,
  background: "var(--bg-input, var(--bg-card))",
  border: "1px solid var(--border)",
  borderRadius: 6,
  padding: "0 8px",
  color: "var(--text-primary)",
  fontFamily: "var(--font-ui)",
  fontSize: "var(--text-lg)",
  cursor: "pointer",
  outline: "none",
};

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
        fontSize: "var(--text-sm)",
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
            fontSize: "var(--text-lg)",
            color: "var(--text-primary)",
            fontWeight: 500,
          }}
        >
          {label}
        </div>
        {description && (
          <div
            style={{
              fontSize: "var(--text-md)",
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
            fontSize: "var(--text-lg)",
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

/** Text-size control. Applies immediately — the whole app resizes live, so
 * the effect of each step is visible while choosing it, including on this
 * control's own label.
 *
 * A dropdown rather than the segmented buttons this used to be: four steps
 * side by side crowded the row, and they grow with the very setting they
 * set, so the widest choice made the control widest exactly when the user
 * had least room. A closed list of ordered steps is what a select is for.
 */
function TextSizeToggle() {
  const [scale, setScale] = useState(readTextScale);
  return (
    <select
      aria-label="Text size"
      value={String(scale)}
      onChange={(e) => {
        const next = parseTextScale(e.target.value);
        setTextScale(next);
        setScale(next);
      }}
      style={SETTINGS_SELECT}
    >
      {TEXT_SCALES.map((option) => (
        <option key={option.label} value={String(option.value)}>
          {option.label}
        </option>
      ))}
    </select>
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
                : updater.phase === "installing"
                  ? `Installing version ${updater.version}…`
                  : updater.phase === "installedNeedsRestart"
                    ? `Version ${updater.version} is installed. Reopen Hydra to finish.`
                    : updater.phase === "error"
                      ? `Update failed: ${updater.message}`
                      : "Check if a newer version of Hydra is available.";

  // `installing` is included: the button sits over an installer that is already
  // running, and a second press would start a second one.
  const busy =
    updater.phase === "checking" ||
    updater.phase === "downloading" ||
    updater.phase === "installing" ||
    updater.phase === "installedNeedsRestart";
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
            : updater.phase === "installing"
              ? "Installing…"
              : updater.phase === "installedNeedsRestart"
                ? "Reopen to finish"
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
          fontSize: "var(--text-lg)",
          fontFamily: "var(--font-ui)",
          fontVariantNumeric: "tabular-nums",
        }}
      >
        {label}
      </button>
    </SettingRow>
  );
}

const UNIT_OPTIONS: Array<{ value: UnitPreference; label: string }> = [
  // "Source" first because it is the default and the least surprising: it
  // shows each model in the system its own file declares, which is also
  // what reports use.
  { value: "source", label: "Source" },
  { value: "si", label: "SI (metric)" },
  { value: "us", label: "US customary" },
];

/** Matches the text-size control beside it: one closed list of named
 * choices reads the same way as another, and three buttons in a row said
 * nothing the dropdown does not. */
function UnitSystemToggle() {
  const unitSystem = useUnitPreference();
  return (
    <select
      aria-label="Default display units"
      value={unitSystem}
      onChange={(e) => setUnitPreference(e.target.value as UnitPreference)}
      style={SETTINGS_SELECT}
    >
      {UNIT_OPTIONS.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  );
}

/**
 * Settings, as a drawer over whatever is underneath.
 *
 * It was a page, and being a page cost it twice: it joined navigation
 * history, so Back walked through settings visits, and arriving at it
 * counted as leaving your project, which erased what the next launch would
 * have reopened. Both followed from calling a detour a destination.
 *
 * As an overlay the page beneath is untouched — open it mid-project,
 * change a unit, dismiss, and you are exactly where you were. `Page` no
 * longer has a `"settings"` member, so neither problem is expressible.
 *
 * The panel is full-height and right-aligned rather than centred: the
 * content is a long single column of rows, which a centred dialog would
 * either crop or float in a lot of empty width.
 */
export function SettingsDrawer() {
  const { settingsOpen, closeSettings } = useAppState();

  // Dev-only: time from the drawer mounting to the next painted frame, as
  // `[hydra-perf] settings-open`. The chunk is 10 kB and prefetched, and
  // everything the body does on mount is async — so if opening still feels
  // slow, this says whether the cost is in the render or somewhere before
  // it, rather than leaving it to impression.
  useEffect(() => {
    if (!import.meta.env.DEV || !settingsOpen) return;
    const span = startPerfSpan("settings-open");
    let inner: number | null = null;
    const outer = requestAnimationFrame(() => {
      inner = requestAnimationFrame(() => span.end());
    });
    return () => {
      cancelAnimationFrame(outer);
      if (inner != null) cancelAnimationFrame(inner);
    };
  }, [settingsOpen]);

  if (!settingsOpen) return null;
  return (
    <ModalBackdrop
      onDismiss={closeSettings}
      zIndex={200}
      style={{ justifyContent: "flex-end", alignItems: "stretch" }}
    >
      <div
        {...stopBackdropEvents}
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        style={{
          width: "min(760px, 100vw)",
          height: "100%",
          background: "var(--bg-app)",
          borderLeft: "1px solid var(--border)",
          boxShadow: "var(--shadow-3)",
          display: "flex",
          flexDirection: "column",
          animation: "slideInRight 180ms ease-out",
        }}
      >
        <SettingsBody onClose={closeSettings} />
      </div>
    </ModalBackdrop>
  );
}

function SettingsBody({ onClose }: { onClose: () => void }) {
  // Close on Escape, as every other modal in the app does.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
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
        <div
          style={{
            display: "flex",
            alignItems: "flex-start",
            justifyContent: "space-between",
            gap: 16,
          }}
        >
          <h1
            style={{
              margin: "0 0 4px",
              fontSize: "var(--text-3xl)",
              fontWeight: 700,
              letterSpacing: "-0.015em",
            }}
          >
            Settings
          </h1>
          {/* A drawer needs a visible way out. The backdrop and Escape both
              close it, but neither is discoverable. */}
          <button
            type="button"
            onClick={onClose}
            aria-label="Close settings"
            data-tooltip="Close"
            style={{
              flexShrink: 0,
              marginTop: 6,
              width: 28,
              height: 28,
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              borderRadius: 6,
              border: "1px solid var(--border)",
              background: "transparent",
              color: "var(--text-secondary)",
              cursor: "pointer",
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.background = "var(--nav-hover)";
              e.currentTarget.style.color = "var(--text-primary)";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.background = "transparent";
              e.currentTarget.style.color = "var(--text-secondary)";
            }}
          >
            <XMarkIcon style={{ width: 15, height: 15 }} />
          </button>
        </div>
        <p
          style={{
            margin: "0 0 4px",
            color: "var(--text-secondary)",
            fontSize: "var(--text-xl)",
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
          label="Default display units"
          description="How values are shown and entered, for projects that do not set their own. Source follows each model's declared unit system. Files and exports (INP, CSV, GeoJSON) always remain in the model's native/SI units."
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
              fontSize: "var(--text-lg)",
              fontFamily: "var(--font-ui)",
            }}
          >
            Manage providers…
          </button>
        </SettingRow>
        {/* Accessibility */}
        <Section>Accessibility</Section>
        <SettingRow
          label="Text size"
          description="Scale text throughout the app. Hydra cannot follow your system text-size setting, so this control is independent of it."
        >
          <TextSizeToggle />
        </SettingRow>
        <SettingRow
          label="Reduce motion"
          description="Suppress non-essential animations, including panel transitions and the canvas link animation. Takes precedence over the canvas animation control."
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
        <SettingRow
          label="Data folder"
          description="Projects, scenarios, models and results are stored here, alongside your custom CRS definitions."
        >
          <button
            type="button"
            onClick={() => void openDataFolder()}
            style={{
              padding: "5px 14px",
              border: "1px solid var(--border-hover)",
              borderRadius: 6,
              background: "transparent",
              color: "var(--text-primary)",
              cursor: "pointer",
              fontSize: "var(--text-lg)",
              fontFamily: "var(--font-ui)",
            }}
          >
            Reveal…
          </button>
        </SettingRow>
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
              fontSize: "var(--text-lg)",
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
              fontSize: "var(--text-lg)",
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
