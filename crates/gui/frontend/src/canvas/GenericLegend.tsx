// ── The legend ────────────────────────────────────────────────────────────────
// One component for every engine. Its structure is fixed — a persistent
// glass bar of variable pickers, and a popover of one labelled ramp per
// element class over a single scale control — and everything inside it
// comes from the engine's §6 result catalog: ids, labels, symbols, units,
// ranges, ramp shapes, and the states of categorical variables.
//
// No engine names, variable ids, or palettes are known here. That is the
// point: the legend used to exist twice, once per engine, and the wds copy
// re-declared by hand a variable list the engine had already published —
// so the two could disagree, and did.
//
// Per-engine affordances are supplied by the caller as optional props
// (locating extremes, link animation, criteria bands) rather than branched
// on inside: an engine that has no such notion simply passes nothing and
// the affordance is absent rather than faked.

import { PlayIcon, XMarkIcon } from "@heroicons/react/16/solid";
import { useEffect, useRef, useState } from "react";
import {
  formatGenericValue,
  type GenericResultMeta,
  type GenericVariable,
  genericUnitLabel,
} from "../hooks/results";
import { useUnitSystem } from "../units";
import {
  CategorySwatches,
  CRITERIA_SCALE_OPTION,
  DATA_SCALE_OPTIONS,
  LEGEND_BAR_STYLE,
  LEGEND_POPOVER_STYLE,
  LEGEND_ROOT_STYLE,
  LEGEND_SWATCH_BTN_STYLE,
  LinkGlyph,
  NodeGlyph,
  PICKER_BTN_STYLE,
  PickerButton,
  Ramp,
  RegionGlyph,
  ScaleControl,
  type ScaleMode,
  SECTION_LABEL_STYLE,
} from "./legend-primitives";
import {
  bandedGradientCss,
  categoryRgba,
  divergingGradientCss,
  sequentialGradientCss,
} from "./MapCanvas/colorUtils";

export type GenericClassKey = "point" | "polyline" | "region";

/** Selected variable id per element class ("" = the catalog's first). */
export type GenericSelection = Record<GenericClassKey, string>;

/** CSS gradient for a ramp hint, sampled from the ramp functions themselves
 * rather than restated here — the legend cannot drift from the map if it is
 * drawn from the same code. */
function rampGradient(ramp: GenericVariable["ramp"], cls: string): string {
  if (ramp.type === "diverging") return divergingGradientCss();
  if (ramp.type === "banded") return bandedGradientCss();
  if (ramp.type === "categorical") {
    // Hard stops, so the swatch reads as a set of states rather than a
    // scale with intermediate values.
    const n = ramp.items.length || 1;
    const stops = ramp.items.map((it, i) => {
      const c = categoryRgba(i, 220, it.severity);
      return `rgb(${c[0]},${c[1]},${c[2]}) ${(i * 100) / n}% ${((i + 1) * 100) / n}%`;
    });
    return `linear-gradient(to right, ${stops.join(", ")})`;
  }
  return sequentialGradientCss(cls);
}

/** Picker option label in the active display system: "Depth (ft)". */
function optionLabel(v: GenericVariable, sys: "si" | "us"): string {
  const unit = genericUnitLabel(v.quantity, sys);
  return unit ? `${v.label} (${unit})` : v.label;
}

/** Compact "Locate  min  max" row placed under a variable's ramp. */
function LocateRow({ onLocate }: { onLocate: (which: "min" | "max") => void }) {
  const btn: React.CSSProperties = {
    background: "transparent",
    border: "1px solid var(--border)",
    borderRadius: 4,
    color: "var(--text-secondary)",
    fontSize: "var(--text-xs)",
    fontFamily: "var(--font-ui)",
    padding: "1px 6px",
    cursor: "pointer",
  };
  return (
    <div
      style={{ display: "flex", alignItems: "center", gap: 5, marginTop: 5 }}
    >
      <span
        style={{ fontSize: "var(--text-xs)", color: "var(--text-tertiary)" }}
      >
        Locate
      </span>
      <button
        type="button"
        style={btn}
        onClick={() => onLocate("min")}
        data-tooltip="Select and zoom to the network minimum"
      >
        min
      </button>
      <button
        type="button"
        style={btn}
        onClick={() => onLocate("max")}
        data-tooltip="Select and zoom to the network maximum"
      >
        max
      </button>
    </div>
  );
}

interface ClassConfig {
  key: GenericClassKey;
  variables: GenericVariable[];
  glyph: React.ReactNode;
  pickerLabel: string;
}

/** What the caller must supply for the optional animation control. */
export interface AnimationControl {
  playing: boolean;
  onToggle: (playing: boolean) => void;
  /** Variable ids the animation applies to; others disable the button. */
  appliesTo: readonly string[];
  /** Global "Reduce motion" preference — always wins over the toggle. */
  reducedMotion: boolean;
}

export function GenericLegend({
  meta,
  hasRegions,
  selection,
  onSelect,
  scaleMode,
  onScaleModeChange,
  effectiveRanges,
  criteriaVariables,
  criteriaAnnotation,
  onLocateExtreme,
  animation,
  detailsOpen,
  onDetailsOpenChange,
}: {
  meta: GenericResultMeta;
  /** Whether the canvas is showing region polygons — hides the region
   * picker for models without areal elements. */
  hasRegions: boolean;
  selection: GenericSelection;
  onSelect: (cls: GenericClassKey, id: string) => void;
  /** What the ramps are scaled against. */
  scaleMode: ScaleMode;
  onScaleModeChange: (mode: ScaleMode) => void;
  /** The range each class's ramp actually spans. The popover shows these
   * rather than the catalog's declared ones, so the numbers always say what
   * the colours currently mean. */
  effectiveRanges?: Partial<
    Record<GenericClassKey, { min: number; max: number }>
  >;
  /** Variable ids the project holds criteria bands for. The Criteria scale
   * is offered only while one of them is selected — an engine or a
   * variable with no bands simply never sees the option. */
  criteriaVariables?: readonly string[];
  /** Read-only band text for a variable, shown beneath its ramp. The
   * legend displays criteria; it does not author them (they are project
   * analysis inputs, edited in Analysis). */
  criteriaAnnotation?: (variableId: string) => string | null;
  onLocateExtreme?: (cls: GenericClassKey, which: "min" | "max") => void;
  animation?: AnimationControl;
  /** Whether the ramp popover is showing. Owned by the caller so it
   * survives remounts and is remembered per project — it is a working
   * mode, not a transient menu. */
  detailsOpen: boolean;
  onDetailsOpenChange: (open: boolean) => void;
}) {
  const sys = useUnitSystem();
  const [openPicker, setOpenPicker] = useState<GenericClassKey | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  const setDetailsOpen = onDetailsOpenChange;

  // A pointer press outside the legend dismisses the *pickers* only.
  //
  // A picker is a menu: it asks one question, and pressing elsewhere
  // answers "not this one". The ramp popover is reference material — it is
  // the legend — and panning the map is exactly what you keep a legend
  // open for. It used to be dismissed here too, which closed it on the
  // first drag every time.
  useEffect(() => {
    function onPointerDown(e: PointerEvent) {
      if (rootRef.current?.contains(e.target as Node)) return;
      setOpenPicker(null);
    }
    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, []);

  // Escape is the deliberate dismissal the popover keeps instead: it
  // closes the open picker first, then the popover, so one key never
  // discards two layers at once.
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key !== "Escape") return;
      if (openPicker != null) setOpenPicker(null);
      else if (detailsOpen) setDetailsOpen(false);
    }
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [openPicker, detailsOpen, setDetailsOpen]);

  const classes: ClassConfig[] = [
    {
      key: "point" as const,
      variables: meta.pointVars,
      glyph: <NodeGlyph />,
      pickerLabel: "Node variable",
    },
    {
      key: "polyline" as const,
      variables: meta.polylineVars,
      glyph: <LinkGlyph />,
      pickerLabel: "Link variable",
    },
    ...(hasRegions
      ? [
          {
            key: "region" as const,
            variables: meta.regionVars,
            glyph: <RegionGlyph />,
            pickerLabel: "Region variable",
          },
        ]
      : []),
  ].filter((c) => c.variables.length > 0);

  const selected = (c: ClassConfig): GenericVariable =>
    c.variables.find((v) => v.id === selection[c.key]) ?? c.variables[0];

  // Criteria is offered only when a selected variable actually has bands,
  // so the control never presents a scale that would do nothing.
  const anyCriteria = classes.some((c) =>
    criteriaVariables?.includes(selected(c).id),
  );
  const scaleOptions = anyCriteria
    ? [...DATA_SCALE_OPTIONS, CRITERIA_SCALE_OPTION]
    : DATA_SCALE_OPTIONS;

  // Animation is a property of the moving quantity, not of a class slot:
  // enabled when any currently-selected variable is one the caller says it
  // applies to.
  const animatable =
    animation != null &&
    classes.some((c) => animation.appliesTo.includes(selected(c).id));

  return (
    <div ref={rootRef} style={LEGEND_ROOT_STYLE}>
      {/* ── Popover: one labelled ramp per element class, then the scale ── */}
      {detailsOpen && (
        <div
          className="legend-glass legend-glass--raised"
          style={{ ...LEGEND_POPOVER_STYLE, position: "relative" }}
        >
          {/* Dismissal within reach of where the eye already is. The ramp
              button below reopens it, but reaching back down to the bar to
              close what you are currently reading is a trip the content
              itself should not require. */}
          <button
            type="button"
            onClick={() => setDetailsOpen(false)}
            title="Close"
            aria-label="Close color scale"
            className="tool-btn"
            style={{
              position: "absolute",
              top: 6,
              right: 6,
              width: 18,
              height: 18,
              borderRadius: 5,
              color: "var(--text-tertiary)",
            }}
          >
            <XMarkIcon style={{ width: 11, height: 11 }} />
          </button>

          {classes.map((c, i) => {
            const v = selected(c);
            const range = effectiveRanges?.[c.key] ?? {
              min: v.min,
              max: v.max,
            };
            // Gated on the same list that offers the Criteria scale, so a
            // variable can never be annotated with bands it cannot be
            // scaled by.
            const bands = criteriaVariables?.includes(v.id)
              ? (criteriaAnnotation?.(v.id) ?? null)
              : null;
            return (
              <div key={c.key}>
                <div
                  style={
                    // Only the first label runs alongside the close button;
                    // the rest have the full width.
                    i === 0
                      ? { ...SECTION_LABEL_STYLE, paddingRight: 20 }
                      : SECTION_LABEL_STYLE
                  }
                >
                  {optionLabel(v, sys)}
                </div>
                {v.ramp.type === "categorical" ? (
                  <CategorySwatches
                    items={v.ramp.items.map((it, i) => {
                      const rgb = categoryRgba(i, 220, it.severity);
                      return {
                        label: it.label,
                        color: `rgb(${rgb[0]},${rgb[1]},${rgb[2]})`,
                      };
                    })}
                  />
                ) : (
                  <>
                    <Ramp
                      gradient={rampGradient(v.ramp, c.key)}
                      min={formatGenericValue(
                        range.min,
                        v.quantity,
                        sys,
                        false,
                      )}
                      max={formatGenericValue(
                        range.max,
                        v.quantity,
                        sys,
                        false,
                      )}
                    />
                    {bands && (
                      <div
                        style={{
                          marginTop: 4,
                          fontSize: "var(--text-xs)",
                          color: "var(--text-tertiary)",
                        }}
                      >
                        {bands}
                      </div>
                    )}
                    {onLocateExtreme && (
                      <LocateRow onLocate={(w) => onLocateExtreme(c.key, w)} />
                    )}
                  </>
                )}
              </div>
            );
          })}

          <ScaleControl
            value={scaleMode}
            options={scaleOptions}
            onChange={onScaleModeChange}
          />
        </div>
      )}

      {/* ── Persistent control bar: ramp swatches toggle + variable pickers ── */}
      <div
        className={`legend-glass${
          detailsOpen || openPicker != null ? " legend-glass--raised" : ""
        }`}
        style={LEGEND_BAR_STYLE}
      >
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            setDetailsOpen(!detailsOpen);
            setOpenPicker(null);
          }}
          title="Color scale"
          data-tooltip="Color scale"
          data-tooltip-pos="top"
          className="tool-btn"
          style={LEGEND_SWATCH_BTN_STYLE}
        >
          <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
            {classes.map((c) => (
              <div
                key={c.key}
                style={{
                  width: 28,
                  height: 5,
                  borderRadius: 3,
                  background: rampGradient(selected(c).ramp, c.key),
                }}
              />
            ))}
          </div>
        </button>
        <div className="tool-divider" />
        {classes.map((c) => (
          <PickerButton
            key={c.key}
            value={selected(c).id}
            options={c.variables.map((v) => ({
              value: v.id,
              label: optionLabel(v, sys),
            }))}
            icon={c.glyph}
            pickerLabel={c.pickerLabel}
            isOpen={openPicker === c.key}
            onToggle={() => setOpenPicker((p) => (p === c.key ? null : c.key))}
            onSelect={(id) => {
              onSelect(c.key, id);
              setOpenPicker(null);
            }}
          />
        ))}
        {animation &&
          (() => {
            const disabled = animation.reducedMotion || !animatable;
            const tooltip = animation.reducedMotion
              ? "Animation off (Reduce motion is enabled in Settings)"
              : !animatable
                ? "Animation applies to flow and velocity"
                : animation.playing
                  ? "Pause link animation"
                  : "Play link animation";
            const active = animation.playing && !disabled;
            return (
              <button
                type="button"
                className="tool-btn"
                disabled={disabled}
                onClick={(e) => {
                  e.stopPropagation();
                  animation.onToggle(!animation.playing);
                }}
                title={tooltip}
                data-tooltip={tooltip}
                data-tooltip-pos="top"
                style={{
                  ...PICKER_BTN_STYLE,
                  padding: "4px 8px 4px 6px",
                  // Right corners nest inside the bar's 20px rounding so the
                  // hover fill is never clipped at the bar's rounded end.
                  borderRadius: "6px 16px 16px 6px",
                  color: active
                    ? "var(--accent)"
                    : disabled
                      ? "var(--text-tertiary)"
                      : "var(--text-secondary)",
                  opacity: disabled ? 0.5 : 1,
                  cursor: disabled ? "default" : "pointer",
                }}
              >
                <PlayIcon style={{ width: 12, height: 12 }} />
              </button>
            );
          })()}
      </div>
    </div>
  );
}
