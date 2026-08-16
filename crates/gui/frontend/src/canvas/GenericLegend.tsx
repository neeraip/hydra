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

import {
  InformationCircleIcon,
  PlayIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import { useEffect, useRef, useState } from "react";
import {
  formatGenericValue,
  type GenericResultMeta,
  type GenericVariable,
  genericUnitLabel,
} from "../hooks/results";
import { useUnitSystem } from "../units";
import { type CanvasLayers, useCanvasLayers } from "./layers-context";
import {
  CategorySwatches,
  CriteriaCheckbox,
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
  rampReadingAt,
  rampScaleOf,
  ScaleControl,
  type ScaleMode,
  SECTION_LABEL_STYLE,
} from "./legend-primitives";
import { animationAppliesHint } from "./linkPulse";
import {
  bandedGradientCss,
  categoryRgba,
  divergingGradientCss,
  hardStopGradient,
  NO_RESULT_RGBA,
  sequentialGradientCss,
} from "./MapCanvas/colorUtils";
import {
  effectiveScaleMode,
  scaleControlShown,
  scaleOptions,
} from "./scaleOptions";

export type GenericClassKey = "point" | "polyline" | "region";

/** Selected variable id per element class ("" = the catalog's first). */
export type GenericSelection = Record<GenericClassKey, string>;

/** CSS gradient for a ramp hint, sampled from the ramp functions themselves
 * rather than restated here — the legend cannot drift from the map if it is
 * drawn from the same code. */
function rampGradient(
  ramp: GenericVariable["ramp"],
  cls: string,
  /** The colours this variable is actually painted in, when the caller
   * knows them. Banded variables are not painted alike across engines,
   * and assuming one palette is how the legend came to advertise an
   * orange scale over a map of reds, greens and blues. */
  bands?: readonly string[] | null,
  /** The run's own range. A diverging gradient is clipped to it, so the
   *  bar shows the colours the map actually uses and its end labels are
   *  true of its edges. */
  min = -1,
  max = 1,
): string {
  if (ramp.type === "banded" && bands && bands.length > 0) {
    return hardStopGradient(bands);
  }
  if (ramp.type === "diverging") return divergingGradientCss(min, max);
  // A banded variable the caller supplied no colours for is not being
  // judged right now — it is painted as a magnitude, and the legend says
  // so rather than showing bands the map is not using.
  if (ramp.type === "banded" && bands === null)
    return sequentialGradientCss(cls);
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

/**
 * The engine's own names for the variables its motion can carry.
 *
 * Read off the catalog being displayed rather than from a list written
 * anywhere else, and in catalog order — which is the order the pickers
 * show, so the sentence reads in the order the reader's eye already has.
 * Ids the catalog does not publish are dropped rather than named: a
 * registry entry naming one variable too many should read as a shorter
 * sentence, not as an offer of something that is not there.
 */
function animatedVariableLabels(
  classes: readonly { variables: readonly GenericVariable[] }[],
  appliesTo: readonly string[],
): string[] {
  const seen = new Set<string>();
  const labels: string[] = [];
  for (const cls of classes) {
    for (const v of cls.variables) {
      if (!appliesTo.includes(v.id) || seen.has(v.id)) continue;
      seen.add(v.id);
      labels.push(v.label);
    }
  }
  return labels;
}

/**
 * What motion means on this map, in the engine's own words.
 *
 * The point is not which variables the feature supports — it is what the
 * pulse is claiming, so a reader wondering why depth sits still has the
 * rule rather than a list. The toggle's tooltip names the animated
 * variables, which labels the outcome without teaching anything, and is
 * reachable only by hovering a control you are not looking at.
 *
 * It read "Motion shows rates", which was true of the engine it was
 * written for and false of the other. Water distribution animates Status,
 * a categorical state, and Quality, a concentration the water carries —
 * neither is a rate, and the sentence went on to say that a variable
 * measuring a state stays still while listing one that does not. The rule
 * that survives all three pulse kinds — a rate keeping pace with the
 * colour, something being carried, and whether anything is moving at all —
 * is that the motion is about the water rather than about the reading.
 *
 * Deliberately does *not* characterise what is left out. Most of it is
 * states, but not all — a rate can be unanimated for other reasons, as
 * demand is here — so the closing clause says only that the rest are
 * still, which is a fact about this map rather than a claim about those
 * variables.
 *
 * Empty when nothing this catalog publishes animates: there is no rule to
 * explain where there is no motion.
 */
export function motionExplanation(
  classes: readonly { variables: readonly GenericVariable[] }[],
  appliesTo: readonly string[],
): string {
  const names = animatedVariableLabels(classes, appliesTo);
  if (names.length === 0) return "";
  const list = new Intl.ListFormat("en", {
    style: "long",
    type: "conjunction",
  }).format(names);
  return `Motion follows the water — ${list}. Anything else on this map is a still reading.`;
}

/**
 * Whether one class's *current* selection is actually moving on the canvas.
 *
 * The toggle says what the reader wants — "animate whenever it can" — and
 * keeps saying it whether or not this moment allows it. This says whether
 * the wish is being granted for this class right now, which is a different
 * question and belongs next to the variable that answers it. Before, the
 * two were collapsed into one disabled button: picking a state variable
 * greyed out the control, which read as "you may not want this" rather
 * than "this variable does not move".
 */
export function classIsAnimating(
  animation: AnimationControl | undefined,
  selectedId: string,
): boolean {
  if (!animation?.playing || animation.reducedMotion) return false;
  return animation.appliesTo.includes(selectedId);
}

/** The canvas layer each legend class colours — the toolbar's "Toggle
 * nodes / links / subcatchments". */
const LAYER_FOR_CLASS: Record<GenericClassKey, keyof CanvasLayers> = {
  point: "nodes",
  polyline: "links",
  region: "regions",
};

/**
 * Whether the elements this class colours are hidden on the canvas.
 *
 * A variable picked for a hidden class is still a real choice — it is what
 * the map will show the moment the layer comes back — so the control keeps
 * working and reads as inert rather than disappearing or refusing. Without
 * this, a legend picker beside an empty canvas looked exactly like one
 * beside a coloured one, and a reader who had turned nodes off had nothing
 * on screen to remind them why the node ramp explained nothing.
 */
export function classIsHidden(
  cls: GenericClassKey,
  layers: CanvasLayers,
): boolean {
  return !layers[LAYER_FOR_CLASS[cls]];
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
  /**
   * Variable ids the animation applies to; others disable the button.
   *
   * Engine-specific, and so supplied per engine rather than written here.
   * The sentence shown to a reader whose selection is not one of them used
   * to be supplied alongside — and every engine was handed the water
   * distribution one, so a drainage map offered "Flow, Velocity, Unit
   * headloss, Quality and Status" for a catalog holding four variables,
   * three of those names among them. It is built from these ids and this
   * legend's own catalog now, so it can only ever be in the words of the
   * engine being looked at.
   */
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
  criteriaScale,
  onCriteriaScaleChange,
  effectiveRanges,
  criteriaVariables,
  multiStep,
  criteriaAnnotation,
  bandColors,
  bandEdges,
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
  /** Which classes colour by verdict rather than magnitude, for a class
   *  whose selected variable has criteria. Per class because both engines
   *  band variables in two of them and the readings are independent. */
  criteriaScale?: Partial<Record<GenericClassKey, boolean>>;
  onCriteriaScaleChange?: (cls: GenericClassKey, on: boolean) => void;
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
  /** False when the run produced a single reporting step. */
  multiStep?: boolean;
  /** Read-only band text for a variable, shown beneath its ramp. The
   * legend displays criteria; it does not author them (they are project
   * analysis inputs, edited in Analysis). */
  criteriaAnnotation?: (variableId: string) => string | null;
  /** The colours a banded variable is actually painted in, ascending, or
   * null to use the shared banded ramp. */
  bandColors?: (variableId: string) => string[] | null;
  /** The criterion's cut values for a banded variable, ascending — what
   * the equal-width segments of its bar stand for. Lets a hover read back
   * the region it is over; without them a position on a banded bar names
   * a colour and nothing else. */
  bandEdges?: (variableId: string) => number[] | null;
  onLocateExtreme?: (cls: GenericClassKey, which: "min" | "max") => void;
  animation?: AnimationControl;
  /** Whether the ramp popover is showing. Owned by the caller so it
   * survives remounts and is remembered per project — it is a working
   * mode, not a transient menu. */
  detailsOpen: boolean;
  onDetailsOpenChange: (open: boolean) => void;
}) {
  const sys = useUnitSystem();
  // Read here rather than passed: the toggles are canvas state, and the
  // legend sits inside the same provider the canvas does. The context's
  // default is everything-visible, so a legend rendered outside one dims
  // nothing.
  const { layers: canvasLayers } = useCanvasLayers();
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

  /**
   * Whether `variableId` in class `key` is being judged against criteria
   * right now — it has thresholds *and* the switch beside it is on.
   *
   * Named because two places colour a ramp: the popover's bars and the
   * swatch on the button that opens it. They must agree, and they did
   * not: the swatch asked only whether thresholds existed, so a variable
   * with an unticked box showed magnitude colours in the popover and
   * criteria colours on the button.
   */
  const judgingCriteria = (variableId: string, key: GenericClassKey) =>
    (criteriaVariables?.includes(variableId) ?? false) &&
    (criteriaScale?.[key] ?? false);

  // Same rule one step further: a steady-state run has one reporting step,
  // so scaling to *this* step and across the *whole run* are one scale, and
  // offering both is offering a choice with one outcome.
  const options = scaleOptions(multiStep !== false);
  const activeScale = effectiveScaleMode(scaleMode, options);

  // Animation is a property of the moving quantity, not of a class slot:
  // enabled when any currently-selected variable is one the caller says it
  // applies to.
  const animatable =
    animation != null &&
    classes.some((c) => animation.appliesTo.includes(selected(c).id));
  // Named from the catalog on screen, so the sentence is in this engine's
  // words and can only list variables it actually publishes.
  const appliesToHint = animation
    ? animationAppliesHint(animatedVariableLabels(classes, animation.appliesTo))
    : "";
  const motionHint = animation
    ? motionExplanation(classes, animation.appliesTo)
    : "";

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
            // Whether this variable *can* be judged, and whether it is.
            // The thresholds are shown only while they are in force: read
            // beneath a magnitude ramp they describe a colouring that is
            // not on screen.
            const judgeable = criteriaVariables?.includes(v.id) ?? false;
            const judging = judgingCriteria(v.id, c.key);
            const bands = judging ? (criteriaAnnotation?.(v.id) ?? null) : null;
            return (
              <div key={c.key}>
                {/* The variable's name, and — where it has thresholds —
                    the switch that judges it against them. Beside the
                    thing it applies to rather than in the scale row: it
                    is a property of this variable, and both engines band
                    variables in two classes at once. */}
                <div
                  style={{
                    display: "flex",
                    alignItems: "baseline",
                    justifyContent: "space-between",
                    gap: 8,
                    // Only the first row runs alongside the close button;
                    // the rest have the full width.
                    ...(i === 0
                      ? { ...SECTION_LABEL_STYLE, paddingRight: 20 }
                      : SECTION_LABEL_STYLE),
                  }}
                >
                  <span>{optionLabel(v, sys)}</span>
                  {judgeable && onCriteriaScaleChange && (
                    <CriteriaCheckbox
                      on={criteriaScale?.[c.key] ?? false}
                      onChange={(on) => onCriteriaScaleChange(c.key, on)}
                    />
                  )}
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
                      animating={classIsAnimating(animation, v.id)}
                      // A judged bar is labelled at the seams between its
                      // bands, not at the run's extremes: its segments are
                      // regions, and the data range belongs to a different
                      // axis. One cut more than there are seams would
                      // over-run the bar, so the positions come from the
                      // same count the gradient is built with.
                      boundaries={(() => {
                        const cuts = judging ? bandEdges?.(v.id) : null;
                        if (!cuts || cuts.length === 0) return undefined;
                        const regions = cuts.length + 1;
                        return cuts.map((cut, i) => ({
                          at: (i + 1) / regions,
                          label: formatGenericValue(
                            cut,
                            v.quantity,
                            sys,
                            false,
                          ),
                        }));
                      })()}
                      // Reading a colour off the bar: the caller owns this
                      // because the scale, the unit system and the
                      // quantity's formatting are all its knowledge.
                      // Null where a position names a band rather than a
                      // value — see `rampScaleOf`.
                      readAt={(t) => {
                        const scale = rampScaleOf(
                          v.ramp,
                          range.min,
                          range.max,
                          judging ? (bandEdges?.(v.id) ?? null) : null,
                        );
                        if (!scale) return null;
                        const show = (n: number) =>
                          formatGenericValue(n, v.quantity, sys);
                        const reading = rampReadingAt(scale, t);
                        if (reading.kind === "value")
                          return show(reading.value);
                        // A band names a region, so it reads as one — open
                        // at the ends, where there is no further cut.
                        if (reading.from === null && reading.to !== null)
                          return `< ${show(reading.to)}`;
                        if (reading.to === null && reading.from !== null)
                          return `≥ ${show(reading.from)}`;
                        return reading.from !== null && reading.to !== null
                          ? `${show(reading.from)} – ${show(reading.to)}`
                          : null;
                      }}
                      gradient={rampGradient(
                        v.ramp,
                        c.key,
                        judging ? bandColors?.(v.id) : null,
                        range.min,
                        range.max,
                      )}
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

          {scaleControlShown(options) && (
            <ScaleControl
              value={activeScale}
              options={options}
              onChange={onScaleModeChange}
            />
          )}

          {/* The foot of the panel: what the grey means, and — behind the
              mark — why some selections move. The motion note was three
              lines of standing explanation here, read once and in the way
              ever after.

              The key the map most often raises and the legend never
              answered: an element the results do not report for this
              variable — a pump with no velocity, an element absent from a
              period — is drawn in this grey. Read as a colour on the ramp
              it means nothing; named, it is the answer to "why is that one
              plain". Left of the mark, because it is a legend entry rather
              than an aside. */}
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              gap: 8,
            }}
          >
            <span
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 5,
                fontSize: "var(--text-xs)",
                color: "var(--text-tertiary)",
              }}
            >
              {/* The map's own constant, so the swatch cannot drift from
                  the colour it stands for. */}
              <span
                style={{
                  width: 10,
                  height: 10,
                  borderRadius: 3,
                  flexShrink: 0,
                  background: `rgb(${NO_RESULT_RGBA[0]}, ${NO_RESULT_RGBA[1]}, ${NO_RESULT_RGBA[2]})`,
                }}
              />
              Not reported
            </span>
            {motionHint && (
              <>
                {/* The sentence is rendered, not merely attached: a
                    tooltip is pointer-only here, so a screen reader would
                    otherwise get an icon and nothing to read. */}
                <span className="sr-only">{motionHint}</span>
                <span
                  className="tool-btn"
                  data-tooltip={motionHint}
                  data-tooltip-pos="top"
                  aria-hidden="true"
                  style={{
                    width: 18,
                    height: 18,
                    borderRadius: 5,
                    color: "var(--text-tertiary)",
                    display: "inline-flex",
                    alignItems: "center",
                    justifyContent: "center",
                    cursor: "help",
                  }}
                >
                  <InformationCircleIcon style={{ width: 12, height: 12 }} />
                </span>
              </>
            )}
          </div>
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
          aria-label="Color scale"
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
                  background: rampGradient(
                    selected(c).ramp,
                    c.key,
                    judgingCriteria(selected(c).id, c.key)
                      ? bandColors?.(selected(c).id)
                      : null,
                  ),
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
            animating={classIsAnimating(animation, selected(c).id)}
            dimmed={classIsHidden(c.key, canvasLayers)}
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
            // Only a global "reduce motion" can refuse the wish; which
            // variable happens to be selected cannot. A toggle that greys
            // out when you pick a state variable reads as "you may not
            // want this", when the truth is "this variable does not move"
            // — which the pickers now say, beside the variable itself.
            const disabled = animation.reducedMotion;
            const tooltip = animation.reducedMotion
              ? "Animation off (Reduce motion is enabled in Settings)"
              : !animatable
                ? appliesToHint
                : animation.playing
                  ? "Pause animation"
                  : "Play animation";
            // The fill shows the standing wish, not this moment's outcome:
            // it stays lit over a variable that does not move, because the
            // setting has not changed.
            const active = animation.playing && !disabled;
            // The button's own fill, which hover composites on top of
            // rather than replacing: `--selection-bg-strong` is three times
            // the weight of `--nav-hover`, so swapping one for the other
            // would *dim* a lit button under the pointer. Stacking the
            // hover tint as a gradient layer lifts both states, and uses
            // only tokens, so it follows the theme.
            const fill = active ? "var(--selection-bg-strong)" : "transparent";
            const hoverFill = `linear-gradient(var(--nav-hover), var(--nav-hover)), ${fill}`;
            return (
              <button
                type="button"
                className="tool-btn"
                disabled={disabled}
                onClick={(e) => {
                  e.stopPropagation();
                  animation.onToggle(!animation.playing);
                }}
                aria-label={tooltip}
                data-tooltip={tooltip}
                data-tooltip-pos="top"
                // Inline styles beat the stylesheet, so `.tool-btn:hover`
                // could never show through the state fill set below.
                onMouseEnter={(e) => {
                  if (disabled) return;
                  e.currentTarget.style.background = hoverFill;
                  if (!active) {
                    e.currentTarget.style.color = "var(--text-primary)";
                  }
                }}
                onMouseLeave={(e) => {
                  if (disabled) return;
                  e.currentTarget.style.background = fill;
                  if (!active) {
                    e.currentTarget.style.color = "var(--text-secondary)";
                  }
                }}
                style={{
                  ...PICKER_BTN_STYLE,
                  padding: "4px 6px 4px 6px",
                  // Right corners nest inside the bar's 20px rounding so the
                  // hover fill is never clipped at the bar's rounded end.
                  borderRadius: "6px 16px 16px 6px",
                  // On reads as filled, off as empty — the same language
                  // every other toggle in this bar uses. Dimming the icon
                  // instead made "off" and "unavailable" the same picture,
                  // and left the on-state legible only by hue.
                  background: fill,
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
