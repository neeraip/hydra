import { lazy, Suspense, useEffect, useState } from "react";
import { useAppState } from "../../AppContext";
import { getVersions, openDataFolder, type Versions } from "../../hooks";
import { formatIpcError } from "../../hooks/ipc";
import {
  clearAllResults,
  type DataUsage,
  describeCleared,
  describeUsage,
  getDataUsage,
  openLogFolder,
} from "../../hooks/storage";
import {
  readAutoUpdateCheck,
  setAutoUpdateCheck,
  useUpdater,
} from "../../hooks/useUpdater";
import { loadLicensesModal } from "../../lazyChunks";
import { resetPreferences } from "../../preferences";
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

/**
 * The lines a bug report needs, in one paste.
 *
 * Three facts, each from somewhere else: the app and engine versions come
 * from the backend, the platform is the binary's own target rather than
 * anything the webview knows, and the user agent names the webview, which
 * on macOS and Linux is the system's and not ours. Anyone answering a
 * report asks for all three, and until now the reporter had to find them.
 */
export function diagnosticsText(
  versions: Versions | null,
  userAgent: string,
): string {
  return [
    `Hydra ${versions?.app ?? "unknown"} (engine ${versions?.hydra ?? "unknown"})`,
    `Platform: ${versions?.platform ?? "unknown"}`,
    `Webview: ${userAgent}`,
  ].join("\n");
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

/**
 * The confirm step for an action that cannot be undone.
 *
 * In the row rather than in a dialog: Settings is already an overlay, and
 * a confirmation modal over a drawer over the app is three layers deep to
 * answer a yes-or-no question. The affirmative says what it will do
 * ("Clear them", "Reset everything") rather than "OK", so a reader who
 * arrived at this row by accident is told what they are agreeing to.
 */
function ConfirmPair({
  label,
  onConfirm,
  onCancel,
}: {
  label: string;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div style={{ display: "flex", gap: 6 }}>
      <button type="button" onClick={onCancel} style={ROW_BUTTON}>
        Cancel
      </button>
      <button
        type="button"
        onClick={onConfirm}
        style={{
          ...ROW_BUTTON,
          borderColor: "var(--status-error)",
          color: "var(--status-error)",
        }}
      >
        {label}
      </button>
    </div>
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
  const { openBasemapProvidersModal, toggleShortcutCard } = useAppState();
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
  const [usage, setUsage] = useState<DataUsage | null>(null);
  // What the Data and reset rows say after they have done something. Both
  // actions are invisible otherwise: the folder is smaller, the theme is
  // back to default, and nothing on screen admits to having acted.
  const [dataNote, setDataNote] = useState<string | null>(null);
  const [clearing, setClearing] = useState(false);
  // Separate from `dataNote`: an unreadable log belongs in the log row,
  // not in the sentence about clearing results.
  const [logNote, setLogNote] = useState<string | null>(null);
  // Destructive-ish actions ask once, in place, rather than opening a
  // dialog over a drawer that is itself an overlay.
  const [confirming, setConfirming] = useState<"results" | "prefs" | null>(
    null,
  );
  const [copied, setCopied] = useState(false);

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
    void getDataUsage().then(setUsage);
  }, []);

  async function clearResults() {
    setConfirming(null);
    setClearing(true);
    const cleared = await clearAllResults();
    setClearing(false);
    if (cleared === null) return;
    setDataNote(describeCleared(cleared));
    // Re-measure rather than subtracting: the figure is what is on disk,
    // and a run that finished while the clear was working would make
    // arithmetic disagree with the folder.
    void getDataUsage().then(setUsage);
  }

  function resetToDefaults() {
    setConfirming(null);
    const cleared = resetPreferences();
    // Reload rather than re-applying each preference here: every one of
    // them is read at startup by the module that owns it, and rebuilding
    // that here would be a second, quietly diverging copy of the defaults.
    if (cleared > 0) window.location.reload();
    else setDataNote("Nothing to reset — everything is already at default.");
  }

  async function copyDiagnostics() {
    const text = diagnosticsText(versions, navigator.userAgent);
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error("Could not copy version info:", err);
    }
  }

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
      {/* The card was reachable only from the command palette, which is
          itself a shortcut — so the list of shortcuts was behind knowing
          one. */}
      <SettingRow
        label="Keyboard shortcuts"
        description="Every shortcut Hydra listens for, on one card."
      >
        <button type="button" onClick={toggleShortcutCard} style={ROW_BUTTON}>
          Show…
        </button>
      </SettingRow>
      <SettingRow
        label="Reset preferences"
        description="Put every setting on this page, and how the panels and canvas are arranged, back to default. Your projects, models and results are untouched."
      >
        {confirming === "prefs" ? (
          <ConfirmPair
            label="Reset everything"
            onConfirm={resetToDefaults}
            onCancel={() => setConfirming(null)}
          />
        ) : (
          <button
            type="button"
            onClick={() => setConfirming("prefs")}
            style={ROW_BUTTON}
          >
            Reset…
          </button>
        )}
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
        description={`Projects, scenarios, models and results are stored here, alongside your custom CRS definitions. ${describeUsage(usage)}`}
      >
        <button
          type="button"
          onClick={() => void openDataFolder()}
          style={ROW_BUTTON}
        >
          Reveal…
        </button>
      </SettingRow>
      {/* Results are the only thing offered for deletion: they are derived,
          they are reproducible by running again, and they are most of what
          is on disk. Models and reports are neither. */}
      <SettingRow
        label="Simulation results"
        description={
          dataNote ??
          "Clearing results returns every project to its unsimulated state. The models are kept, so a run puts them back."
        }
      >
        {confirming === "results" ? (
          <ConfirmPair
            label="Clear them"
            onConfirm={() => void clearResults()}
            onCancel={() => setConfirming(null)}
          />
        ) : (
          <button
            type="button"
            disabled={clearing || usage?.resultsBytes === 0}
            onClick={() => setConfirming("results")}
            style={{
              ...ROW_BUTTON,
              color:
                clearing || usage?.resultsBytes === 0
                  ? "var(--text-tertiary)"
                  : "var(--text-primary)",
              cursor:
                clearing || usage?.resultsBytes === 0 ? "default" : "pointer",
            }}
          >
            {clearing ? "Clearing…" : "Clear results…"}
          </button>
        )}
      </SettingRow>
      {/* About */}
      <Section>About</Section>
      {/* Everything the app sends anywhere, in one place. It makes three
          kinds of call and said so nowhere — and one of them carries the
          reader's own map keys and, implicitly, where their network is. */}
      <SettingRow
        label="What Hydra sends"
        description="Hydra asks GitHub for update information and release notes, and requests map tiles from whichever basemap provider you have chosen — those requests carry your key and the area you are looking at. Your models, results and reports are never uploaded anywhere."
      >
        {null}
      </SettingRow>
      {/* Beside the version info, because they are collected for the same
          reason and by the same person. */}
      <SettingRow
        label="Diagnostic log"
        description={
          logNote ??
          "What Hydra recorded while running — the other half of a bug report. One file per day, the last seven kept."
        }
      >
        <button
          type="button"
          onClick={() => {
            openLogFolder().catch((err) => setLogNote(formatIpcError(err)));
          }}
          style={ROW_BUTTON}
        >
          Reveal…
        </button>
      </SettingRow>
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
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            fontSize: "var(--text-lg)",
          }}
        >
          <span style={{ color: "var(--text-secondary)" }}>Platform</span>
          <span style={{ color: "var(--text-primary)" }}>
            {versions?.platform ?? "—"}
          </span>
        </div>
        {/* The "Report a bug" link on the home page opens an issue form
            that asks for all of this. Reading it off three rows and typing
            it back is the kind of work nobody does accurately. */}
        <button
          type="button"
          onClick={() => void copyDiagnostics()}
          style={{ ...ROW_BUTTON, alignSelf: "flex-start", marginTop: 4 }}
        >
          {copied ? "Copied" : "Copy version info"}
        </button>
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
