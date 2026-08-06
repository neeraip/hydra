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
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  // Where to draw the menu, measured from the trigger when it opens.
  //
  // Fixed rather than absolute, for the same reason the scenario pickers
  // in this toolbar are: `ProjectPage` wraps the toolbar in a column with
  // `overflow: hidden`, which clips any child extending past the toolbar's
  // own height — so an absolutely-positioned menu is cut off no matter how
  // high its z-index goes. Fixed positioning leaves that clipping context
  // entirely.
  const [anchor, setAnchor] = useState<{ left: number; top: number } | null>(
    null,
  );

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
    // The anchor is measured, not tracked: the toolbar does not scroll, so
    // the only things that can move it are a resize or a layout change big
    // enough that closing is the honest response.
    function reposition() {
      setOpen(false);
    }
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    window.addEventListener("resize", reposition);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("resize", reposition);
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
    description?: string,
  ) => (
    <button
      key={key}
      type="button"
      role="menuitemradio"
      aria-checked={selected}
      onClick={onClick}
      // Restores to the *selected* colour rather than a constant: the
      // checked row is accent-coloured, and resetting everything to
      // secondary on mouse-out would quietly un-highlight it.
      onMouseEnter={(e) => {
        e.currentTarget.style.background = "var(--nav-hover)";
        e.currentTarget.style.color = "var(--text-primary)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.background = "transparent";
        e.currentTarget.style.color = selected
          ? "var(--accent)"
          : "var(--text-secondary)";
      }}
      style={{
        display: "flex",
        alignItems: "flex-start",
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
        transition: "background var(--t-fast), color var(--t-fast)",
      }}
    >
      <span style={{ width: 12, flexShrink: 0 }}>{selected ? "✓" : ""}</span>
      <span
        style={{
          display: "flex",
          flexDirection: "column",
          gap: 1,
          // Without this a flex child refuses to shrink below its content,
          // so the description stretches the menu instead of wrapping.
          minWidth: 0,
        }}
      >
        {label}
        {description && (
          <span
            style={{
              fontSize: "var(--text-xs)",
              fontWeight: 400,
              color: "var(--text-tertiary)",
              lineHeight: 1.4,
            }}
          >
            {description}
          </span>
        )}
      </span>
    </button>
  );

  /**
   * A group heading, optionally with a hint about what the whole group
   * means.
   *
   * The Default group's explanation rides on its single row instead, where
   * it describes what choosing that row does. Override has three rows and
   * one shared consequence, so it belongs up here rather than repeated
   * three times.
   */
  const groupLabel = (text: string, hint?: string) => (
    <div
      style={{
        padding: "6px 10px 2px",
        fontSize: "var(--text-xs)",
        color: "var(--text-tertiary)",
      }}
    >
      <span style={{ letterSpacing: "0.05em", textTransform: "uppercase" }}>
        {text}
      </span>
      {hint && <span style={{ opacity: 0.85 }}> · {hint}</span>}
    </div>
  );

  return (
    <div ref={wrapRef} style={{ position: "relative", flexShrink: 0 }}>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => {
          const rect = triggerRef.current?.getBoundingClientRect();
          if (rect) setAnchor({ left: rect.left, top: rect.bottom + 4 });
          setOpen((v) => !v);
        }}
        aria-haspopup="menu"
        aria-expanded={open}
        data-tooltip="Display units for this project"
        data-tooltip-pos="bottom"
        // Matches the other toolbar buttons: background, border and text
        // all lift together, and the open menu keeps the lit state so the
        // trigger reads as the thing the popover belongs to.
        onMouseEnter={(e) => {
          e.currentTarget.style.background = "var(--nav-hover)";
          e.currentTarget.style.borderColor = "var(--border-hover)";
          e.currentTarget.style.color = "var(--text-primary)";
        }}
        onMouseLeave={(e) => {
          e.currentTarget.style.background = open
            ? "var(--nav-hover)"
            : "transparent";
          e.currentTarget.style.borderColor = "var(--border)";
          e.currentTarget.style.color = "var(--text-secondary)";
        }}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 6,
          padding: "3px 8px",
          borderRadius: 6,
          border: "1px solid var(--border)",
          background: open ? "var(--nav-hover)" : "transparent",
          color: "var(--text-secondary)",
          fontFamily: "var(--font-ui)",
          fontSize: "var(--text-sm)",
          cursor: "pointer",
          transition:
            "background var(--t-fast), border-color var(--t-fast), color var(--t-fast)",
        }}
      >
        {SYSTEM_LABEL[effective]}
        {/* Why it is what it is, without opening the menu — and the answer
            to "I changed Settings, why did this project not follow?" */}
        {inherited && (
          <span style={{ color: "var(--text-tertiary)" }}>· Default</span>
        )}
      </button>

      {open && anchor && (
        <div
          role="menu"
          style={{
            position: "fixed",
            top: anchor.top,
            left: anchor.left,
            // Matches the toolbar's other popovers, which sit above the
            // canvas and the secondary rail.
            zIndex: 120,
            width: 244,
            padding: "4px 0",
            borderRadius: 8,
            border: "1px solid var(--border)",
            background: "var(--bg-panel)",
            boxShadow: "var(--shadow-2)",
          }}
        >
          {groupLabel("Default")}
          {/* Says where the value came from, which the label alone cannot:
              the row shows the *resolved* system, so without this it reads
              as a third explicit choice rather than as deference to
              Settings.

              "Follows" against the override group's "Fixed" is the whole
              distinction in two words apiece — which is what these rows
              need, since they are identical in text whenever the default
              is Source. */}
          {row(
            appDefault === "source"
              ? sourceOptionLabel(modelSystem)
              : SYSTEM_LABEL[appDefault],
            inherited,
            () => choose(null),
            "inherit",
            "Follows your app Settings",
          )}
          <div
            style={{
              height: 1,
              margin: "4px 0",
              background: "var(--border)",
            }}
          />
          {/* "Fixed" against the Default row's "Follows" is what makes the
              duplicate `Source` entry above meaningful: these stay put when
              Settings moves. */}
          {groupLabel("Override", "fixed for this project")}
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
