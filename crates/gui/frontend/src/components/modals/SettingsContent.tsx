import { lazy, Suspense, useEffect, useState } from "react";
import { useAppState } from "../../AppContext";
import { getVersions, openDataFolder, type Versions } from "../../hooks";
import {
  readAutoUpdateCheck,
  setAutoUpdateCheck,
  useUpdater,
} from "../../hooks/useUpdater";
import { loadLicensesModal } from "../../lazyChunks";
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
import { Toggle } from "../ui/Toggle";
import type { LicenseTab } from "./LicensesModal";
import { Section, SettingRow } from "./SettingsPrimitives";

const LicensesModal = lazy(() =>
  loadLicensesModal().then((m) => ({ default: m.LicensesModal })),
);

/** The row controls all look like this — a bordered button that opens
 *  something. Written once so a new one cannot arrive slightly different. */
const ROW_BUTTON: React.CSSProperties = {
  padding: "5px 14px",
  border: "1px solid var(--border-hover)",
  borderRadius: 6,
  background: "transparent",
  color: "var(--text-primary)",
  cursor: "pointer",
  fontSize: "var(--text-lg)",
  fontFamily: "var(--font-ui)",
};

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

/** The update rows: whether to look on launch, and the manual check with
 * its inline status. Hidden entirely when this install can't self-update
 * (dev builds, Linux deb/rpm — those update via the package manager). The
 * status row's description doubles as the status line; the single button
 * carries the whole flow. */
function UpdatesRow() {
  const { updater, supported, install, restart, checkNow } = useUpdater();
  const [autoCheck, setAutoCheckRaw] = useState(readAutoUpdateCheck);
  if (supported !== true) return null;

  const setAutoCheck = (v: boolean) => {
    setAutoCheckRaw(v);
    setAutoUpdateCheck(v);
  };

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
    <>
      <SettingRow
        label="Check for updates automatically"
        description="Ask GitHub whether a newer version exists when Hydra starts. Turning this off leaves the check to you — the button below still works."
      >
        <Toggle checked={autoCheck} onChange={setAutoCheck} />
      </SettingRow>
      <SettingRow label="Software updates" description={description}>
        <button
          type="button"
          disabled={busy}
          onClick={onClick}
          style={{
            ...ROW_BUTTON,
            color: busy ? "var(--text-tertiary)" : "var(--text-primary)",
            cursor: busy ? "default" : "pointer",
            fontVariantNumeric: "tabular-nums",
          }}
        >
          {label}
        </button>
      </SettingRow>
    </>
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
export function SettingsContent() {
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
  // Which tab the licences panel opens on, or null while it is closed —
  // the two rows below open the same panel at different pages.
  const [licenseTab, setLicenseTab] = useState<LicenseTab | null>(null);

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

  // The drawer owns the panel, the width and the header; this is only the
  // rows inside them. Fragment rather than another wrapper so the two files
  // do not each contribute a layout box to the same column.
  return (
    <>
      {/* General */}
      <Section>General</Section>
      <SettingRow
        label="Reopen last project on launch"
        description="Start Hydra straight back in the project you last had open."
      >
        <Toggle checked={restoreSession} onChange={setRestoreSession} />
      </SettingRow>
      {/* Units sat under Appearance, which read them as a look. They are
          not: this setting decides what number you type into a diameter
          field and what a report says the answer is. That is a convention
          you work in, which is what General is for. */}
      <SettingRow
        label="Default display units"
        description="How values are shown and entered, for projects that do not set their own. Source follows each model's declared unit system. Files and exports (INP, CSV, GeoJSON) always remain in the model's native/SI units."
      >
        <UnitSystemToggle />
      </SettingRow>
      {/* Appearance */}
      <Section>Appearance</Section>
      <SettingRow label="Theme" description="Choose dark or light mode.">
        <ThemeToggle />
      </SettingRow>
      <SettingRow
        label="Basemap providers"
        description="Connect imagery providers (Mapbox, MapTiler, Esri) and choose which basemap styles appear in the canvas picker."
      >
        <button
          type="button"
          onClick={openBasemapProvidersModal}
          style={ROW_BUTTON}
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
      {/* Data — where the work lives. It was the one storage row in About,
          a section that otherwise answers "what is this program": version,
          licence, source. A folder full of your projects is not that. */}
      <Section>Data</Section>
      <SettingRow
        label="Data folder"
        description="Projects, scenarios, models and results are stored here, alongside your custom CRS definitions."
      >
        <button
          type="button"
          onClick={() => void openDataFolder()}
          style={ROW_BUTTON}
        >
          Reveal…
        </button>
      </SettingRow>
      {/* About */}
      <Section>About</Section>
      <UpdatesRow />
      <SettingRow
        label="Licence"
        description="Hydra is free software under the GNU Affero General Public License v3. Using it, and everything it produces, is yours to do as you like with; sharing Hydra's code inside your own product asks the same licence of that product."
      >
        <button
          type="button"
          onClick={() => setLicenseTab("hydra")}
          style={ROW_BUTTON}
        >
          View licence…
        </button>
      </SettingRow>
      <SettingRow
        label="Open-source components"
        description="The licences and copyright notices of every open-source package Hydra is built from, as those licences ask."
      >
        <button
          type="button"
          onClick={() => setLicenseTab("components")}
          style={ROW_BUTTON}
        >
          View notices…
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
      {/* Above the drawer it was opened from: it is a document to read, so
          it takes the middle of the window rather than a column of rows.
          No fallback while the chunk loads — the drawer stays where it is
          and the panel arrives over it. */}
      {licenseTab !== null && (
        <Suspense fallback={null}>
          <LicensesModal tab={licenseTab} onClose={() => setLicenseTab(null)} />
        </Suspense>
      )}
    </>
  );
}
