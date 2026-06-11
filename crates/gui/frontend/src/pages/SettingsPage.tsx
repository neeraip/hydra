import { useEffect, useState } from "react";
import { useAppState } from "../AppContext";
import { Toggle } from "../components/ui/Toggle";
import { getVersions, reconcileProjects, type Versions } from "../hooks";

const SK = {
  reducedMotion: "hydra2-reduced-motion",
  highContrast: "hydra2-high-contrast",
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

export function SettingsPage() {
  const { showToast } = useAppState();
  const [reducedMotion, setReducedMotionRaw] = useState(() =>
    getBool(SK.reducedMotion, false),
  );
  const [highContrast, setHighContrastRaw] = useState(() =>
    getBool(SK.highContrast, false),
  );
  const [versions, setVersions] = useState<Versions | null>(null);
  const [isReconciling, setIsReconciling] = useState(false);

  // Wrap setters to also persist to localStorage.
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
    getVersions().then(setVersions);
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
        {/* Appearance */}
        <Section>Appearance</Section>
        <SettingRow label="Theme" description="Choose dark or light mode.">
          <ThemeToggle />
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
        </SettingRow>{" "}
      </div>
    </div>
  );
}
