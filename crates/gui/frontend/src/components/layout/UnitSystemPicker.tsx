/**
 * The project's display-unit control.
 *
 * Sits with the scenario controls rather than beside Simulate: this is a
 * view setting, and next to the run button it would read as "run in these
 * units" — which it must not, since exports, reports and the INP always
 * stay in the model's own system whatever is chosen here.
 *
 * The closed control shows the system in effect, because that is the
 * question someone has while looking at numbers. *Why* it is in effect —
 * inherited from Settings, or pinned on this project — is a marker on the
 * closed control and the grouping inside the menu.
 */

import { useEffect, useRef, useState } from "react";
import { useActiveProject, useAppState } from "../../AppContext";
import { updateProjectUnits, useModelUnitSystem } from "../../hooks";
import {
  resolveUnitSystem,
  type UnitPreference,
  type UnitSystem,
  useUnitPreference,
} from "../../units";

const SYSTEM_LABEL: Record<UnitSystem, string> = {
  si: "SI (metric)",
  us: "US customary",
};

/** "Source (US customary)", or plain "Source" before the model answers. */
export function sourceOptionLabel(modelSystem: UnitSystem | null): string {
  return modelSystem ? `Source (${SYSTEM_LABEL[modelSystem]})` : "Source";
}

/**
 * The label for one override option.
 *
 * `source` is the only indirect one, so it is the only one that needs to
 * say what it resolves to — the two explicit systems are their own answer.
 */
export function overrideOptionLabel(
  value: UnitPreference,
  modelSystem: UnitSystem | null,
): string {
  return value === "source"
    ? sourceOptionLabel(modelSystem)
    : SYSTEM_LABEL[value];
}

export function UnitSystemPicker() {
  const { project } = useActiveProject();
  const { activeScenarioId, bumpProjects } = useAppState();
  const appDefault = useUnitPreference();
  const modelSystem = useModelUnitSystem(project?.id, activeScenarioId);
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    function onDown(e: MouseEvent) {
      if (!wrapRef.current?.contains(e.target as globalThis.Node)) {
        setOpen(false);
      }
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  if (!project) return null;

  const override = project.unitSystem ?? null;
  const effective = resolveUnitSystem(override, appDefault, modelSystem);
  const inherited = override === null;

  async function choose(next: UnitPreference | null) {
    setOpen(false);
    if (!project) return;
    await updateProjectUnits(project.id, next);
    bumpProjects();
  }

  const row = (
    label: string,
    selected: boolean,
    onClick: () => void,
    key: string,
  ) => (
    <button
      key={key}
      type="button"
      role="menuitemradio"
      aria-checked={selected}
      onClick={onClick}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 8,
        width: "100%",
        padding: "5px 10px",
        border: "none",
        background: "transparent",
        color: selected ? "var(--accent)" : "var(--text-secondary)",
        fontFamily: "var(--font-ui)",
        fontSize: "var(--text-md)",
        fontWeight: selected ? 500 : 400,
        cursor: "pointer",
        textAlign: "left",
      }}
    >
      <span style={{ width: 12, flexShrink: 0 }}>{selected ? "✓" : ""}</span>
      {label}
    </button>
  );

  const groupLabel = (text: string) => (
    <div
      style={{
        padding: "6px 10px 2px",
        fontSize: "var(--text-xs)",
        letterSpacing: "0.05em",
        textTransform: "uppercase",
        color: "var(--text-tertiary)",
      }}
    >
      {text}
    </div>
  );

  return (
    <div ref={wrapRef} style={{ position: "relative", flexShrink: 0 }}>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="menu"
        aria-expanded={open}
        data-tooltip="Display units for this project"
        data-tooltip-pos="bottom"
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 6,
          padding: "3px 8px",
          borderRadius: 6,
          border: "1px solid var(--border)",
          background: "transparent",
          color: "var(--text-secondary)",
          fontFamily: "var(--font-ui)",
          fontSize: "var(--text-sm)",
          cursor: "pointer",
        }}
      >
        {SYSTEM_LABEL[effective]}
        {/* Why it is what it is, without opening the menu — and the answer
            to "I changed Settings, why did this project not follow?" */}
        {inherited && (
          <span style={{ color: "var(--text-tertiary)" }}>· Default</span>
        )}
      </button>

      {open && (
        <div
          role="menu"
          style={{
            position: "absolute",
            top: "calc(100% + 4px)",
            left: 0,
            zIndex: 60,
            minWidth: 200,
            padding: "4px 0",
            borderRadius: 8,
            border: "1px solid var(--border)",
            background: "var(--bg-panel)",
            boxShadow: "var(--shadow-key)",
          }}
        >
          {groupLabel("Default")}
          {row(
            appDefault === "source"
              ? sourceOptionLabel(modelSystem)
              : SYSTEM_LABEL[appDefault],
            inherited,
            () => choose(null),
            "inherit",
          )}
          <div
            style={{
              height: 1,
              margin: "4px 0",
              background: "var(--border)",
            }}
          />
          {groupLabel("Override")}
          {(["source", "si", "us"] as const).map((v) =>
            row(
              overrideOptionLabel(v, modelSystem),
              override === v,
              () => choose(v),
              v,
            ),
          )}
        </div>
      )}
    </div>
  );
}
