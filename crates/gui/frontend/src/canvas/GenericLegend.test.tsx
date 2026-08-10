/**
 * @vitest-environment jsdom
 *
 * Regressions for the shared legend. Every one of these corresponds to a
 * defect that reached the running app: the legend is rendered from
 * engine-authored data, so its failures are silent — a wrong swatch or a
 * missing option looks exactly like a deliberate design.
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { GenericResultMeta, GenericVariable } from "../hooks/results";
import {
  type AnimationControl,
  classIsAnimating,
  classIsHidden,
  criteriaScaleOffered,
  GenericLegend,
  motionExplanation,
} from "./GenericLegend";
import { CRITERIA_SCALE_OPTION, DATA_SCALE_OPTIONS } from "./legend-primitives";

const v = (over: Partial<GenericVariable>): GenericVariable => ({
  id: "depth",
  label: "Depth",
  quantity: undefined,
  ramp: { type: "sequential" },
  min: 0,
  max: 1,
  ...over,
});

const meta = (over: Partial<GenericResultMeta> = {}): GenericResultMeta => ({
  pointVars: [v({ id: "depth", label: "Depth" })],
  polylineVars: [v({ id: "flow", label: "Flow" })],
  regionVars: [],
  ...over,
});

function renderLegend(
  props: Partial<Parameters<typeof GenericLegend>[0]> = {},
) {
  const onScaleModeChange = vi.fn();
  const onDetailsOpenChange = vi.fn();
  const utils = render(
    <GenericLegend
      meta={meta()}
      hasRegions={false}
      selection={{ point: "", polyline: "", region: "" }}
      onSelect={vi.fn()}
      scaleMode="run"
      onScaleModeChange={onScaleModeChange}
      detailsOpen
      onDetailsOpenChange={onDetailsOpenChange}
      {...props}
    />,
  );
  return { ...utils, onScaleModeChange, onDetailsOpenChange };
}

describe("GenericLegend", () => {
  // Each selected variable is named twice on purpose — once as the
  // popover's section heading, once on the picker that changes it.
  it("renders a section per element class from the catalog", () => {
    renderLegend();
    expect(screen.getAllByText("Depth").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Flow").length).toBeGreaterThan(0);
  });

  // The engine says which states exist and what they are called. Dropping
  // them left link status drawn as a gradient over status codes.
  it("names every state of a categorical variable", () => {
    renderLegend({
      meta: meta({
        polylineVars: [
          v({
            id: "status",
            label: "Status",
            ramp: {
              type: "categorical",
              items: [
                { value: 2, label: "Closed", severity: "alarm" },
                { value: 3, label: "Open", severity: "nominal" },
              ],
            },
          }),
        ],
      }),
    });
    expect(screen.getByText("Closed")).toBeDefined();
    expect(screen.getByText("Open")).toBeDefined();
  });

  // Criteria bands are a water-distribution compliance standard. A drainage
  // project was briefly offered them, annotated with numbers from another
  // domain, because both engines publish a variable called `flow`.
  it("offers no Criteria scale to an engine with no bands", () => {
    renderLegend({ criteriaVariables: [] });
    expect(screen.queryByText(CRITERIA_SCALE_OPTION.label)).toBeNull();
    // Read from the options themselves rather than spelled out: these
    // labels are wording, and a test that hardcodes them fails on a
    // rename that broke nothing.
    for (const o of DATA_SCALE_OPTIONS) {
      expect(screen.getByText(o.label)).toBeDefined();
    }
  });

  it("offers Criteria when the selected variable has bands", () => {
    renderLegend({
      criteriaVariables: ["flow"],
      selection: { point: "", polyline: "flow", region: "" },
    });
    expect(screen.getByText(CRITERIA_SCALE_OPTION.label)).toBeDefined();
  });

  it("annotates only variables the engine has bands for", () => {
    const annotation = vi.fn(() => "< 0.1 low");
    renderLegend({ criteriaVariables: [], criteriaAnnotation: annotation });
    expect(screen.queryByText("< 0.1 low")).toBeNull();
  });

  // The popover is reference material, not a menu: panning the map is
  // exactly what it stays open for. It used to close on the first drag.
  it("survives a pointer press outside itself", () => {
    const { onDetailsOpenChange } = renderLegend();
    fireEvent.pointerDown(document.body);
    expect(onDetailsOpenChange).not.toHaveBeenCalled();
    expect(screen.getAllByText("Depth").length).toBeGreaterThan(0);
  });

  it("closes on its own close button", () => {
    const { onDetailsOpenChange } = renderLegend();
    fireEvent.click(screen.getByLabelText("Close color scale"));
    expect(onDetailsOpenChange).toHaveBeenCalledWith(false);
  });

  it("closes on Escape", () => {
    const { onDetailsOpenChange } = renderLegend();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onDetailsOpenChange).toHaveBeenCalledWith(false);
  });

  it("selects a scale mode", () => {
    const { onScaleModeChange } = renderLegend();
    const step = DATA_SCALE_OPTIONS.find((o) => o.mode === "step");
    if (!step) throw new Error("no per-step scale to select");
    fireEvent.click(screen.getByText(step.label));
    expect(onScaleModeChange).toHaveBeenCalledWith("step");
  });

  // A class with no catalog variables must not get an empty picker, and a
  // model with no areal elements must not get a region row at all.
  it("omits classes the catalog has nothing for", () => {
    renderLegend({ meta: meta({ regionVars: [v({ id: "rain" })] }) });
    expect(screen.queryByLabelText("Region variable")).toBeNull();
  });
});

/**
 * The sentence a reader sees when the toggle will not do anything.
 *
 * This legend is every engine's. The sentence used to be handed to it as a
 * finished string, and every engine was handed the water distribution one —
 * so a drainage model, whose conduits publish flow, depth, velocity and
 * capacity, offered "Animation applies to Flow, Velocity, Unit headloss,
 * Quality, and Status". Three of those five do not exist in drainage.
 *
 * It is built from the catalog on screen now, which is the only list that
 * cannot be the wrong engine's.
 */
describe("what the animation toggle says it applies to", () => {
  const animation = (appliesTo: readonly string[]) => ({
    playing: false,
    onToggle: vi.fn(),
    appliesTo,
    reducedMotion: false,
  });

  /** A drainage-shaped catalog: no headloss, no quality, no status. */
  const drainage = meta({
    pointVars: [v({ id: "depth", label: "Depth" })],
    polylineVars: [
      v({ id: "flow", label: "Flow" }),
      v({ id: "depth", label: "Depth" }),
      v({ id: "velocity", label: "Velocity" }),
      v({ id: "capacity", label: "Capacity used" }),
    ],
  });

  function hintFor(over: Parameters<typeof renderLegend>[0]) {
    const { container } = renderLegend(over);
    const titles = Array.from(container.querySelectorAll("[title]")).map((el) =>
      el.getAttribute("title"),
    );
    return titles.find((t) => t?.includes("Animation")) ?? "";
  }

  it("names only variables the catalog on screen publishes", () => {
    const hint = hintFor({
      meta: drainage,
      selection: { point: "", polyline: "capacity", region: "" },
      animation: animation(["flow", "velocity"]),
    });
    expect(hint).toBe("Animation applies to Flow and Velocity");
  });

  /** The exact words that used to reach a drainage reader. */
  it("does not offer another engine's variables", () => {
    const hint = hintFor({
      meta: drainage,
      selection: { point: "", polyline: "capacity", region: "" },
      animation: animation(["flow", "velocity", "headloss", "quality"]),
    });
    expect(hint).not.toContain("headloss");
    expect(hint).not.toContain("quality");
  });

  it("says nothing applies when nothing does", () => {
    const hint = hintFor({
      meta: drainage,
      selection: { point: "", polyline: "capacity", region: "" },
      animation: animation([]),
    });
    expect(hint).toBe("Animation does not apply to this model");
  });
});

/**
 * The toggle is a standing wish — "animate whenever it can" — not a report
 * on this moment. It used to grey out whenever the selected variable was a
 * state, which said "you may not want this" when the truth was "this
 * variable does not move". Whether the wish is being granted is a separate
 * question, and it is answered beside the variable that decides it.
 */
describe("the animation toggle", () => {
  const drainage = meta({
    pointVars: [v({ id: "depth", label: "Depth" })],
    polylineVars: [
      v({ id: "flow", label: "Flow" }),
      v({ id: "capacity", label: "Capacity used" }),
    ],
  });

  const animation = (over: Partial<AnimationControl> = {}) => ({
    playing: true,
    onToggle: vi.fn(),
    appliesTo: ["flow"] as readonly string[],
    reducedMotion: false,
    ...over,
  });

  function toggle(over: Parameters<typeof renderLegend>[0]) {
    const { container } = renderLegend(over);
    return Array.from(container.querySelectorAll("button")).find((b) =>
      b.getAttribute("title")?.match(/animation/i),
    );
  }

  it("stays usable over a variable that does not move", () => {
    const btn = toggle({
      meta: drainage,
      selection: { point: "depth", polyline: "capacity", region: "" },
      animation: animation(),
    });
    expect(btn?.disabled).toBe(false);
  });

  it("still yields to Reduce motion, which nothing can grant", () => {
    const btn = toggle({
      meta: drainage,
      selection: { point: "depth", polyline: "flow", region: "" },
      animation: animation({ reducedMotion: true }),
    });
    expect(btn?.disabled).toBe(true);
  });

  it("keeps the wish clicking through", () => {
    const onToggle = vi.fn();
    const btn = toggle({
      meta: drainage,
      selection: { point: "depth", polyline: "capacity", region: "" },
      animation: animation({ onToggle }),
    });
    // Nothing on screen moves, and the setting still changes.
    if (btn) fireEvent.click(btn);
    expect(onToggle).toHaveBeenCalledWith(false);
  });
});

describe("classIsAnimating", () => {
  const on = {
    playing: true,
    onToggle: () => {},
    appliesTo: ["flow", "flooding"],
    reducedMotion: false,
  };

  it("is true only where the wish is actually granted", () => {
    expect(classIsAnimating(on, "flow")).toBe(true);
    expect(classIsAnimating(on, "flooding")).toBe(true);
    // A state variable: the toggle is on, this class is not moving.
    expect(classIsAnimating(on, "capacity")).toBe(false);
  });

  it("is false whenever motion is off, however it was turned off", () => {
    expect(classIsAnimating({ ...on, playing: false }, "flow")).toBe(false);
    expect(classIsAnimating({ ...on, reducedMotion: true }, "flow")).toBe(
      false,
    );
    expect(classIsAnimating(undefined, "flow")).toBe(false);
  });
});

/**
 * A picker for a class the reader has switched off still works — the
 * variable it names is what the map will show the moment the layer comes
 * back — so it dims rather than disappearing or refusing. The rows inside
 * it are another matter: those are as clickable as ever, and dimming them
 * would say the choice itself is unavailable.
 */
describe("classIsHidden", () => {
  const allOn = {
    nodes: true,
    links: true,
    regions: true,
    nodeLabels: false,
    linkLabels: false,
  };

  it("maps each legend class onto the layer it colours", () => {
    expect(classIsHidden("point", { ...allOn, nodes: false })).toBe(true);
    expect(classIsHidden("polyline", { ...allOn, links: false })).toBe(true);
    expect(classIsHidden("region", { ...allOn, regions: false })).toBe(true);
  });

  it("dims only the class that was switched off", () => {
    const noNodes = { ...allOn, nodes: false };
    expect(classIsHidden("polyline", noNodes)).toBe(false);
    expect(classIsHidden("region", noNodes)).toBe(false);
  });

  it("dims nothing when everything is on show", () => {
    for (const cls of ["point", "polyline", "region"] as const) {
      expect(classIsHidden(cls, allOn)).toBe(false);
    }
  });
});

/**
 * Why some selections move and others do not, said where a reader is
 * already looking to find out what the colours mean. The toggle's tooltip
 * named the animated variables, which labels the outcome without teaching
 * the rule — and only to someone who hovers a control they are not
 * looking at while wondering why the map is still.
 */
describe("motionExplanation", () => {
  const drainage = [
    { variables: [v({ id: "depth", label: "Depth" })] },
    {
      variables: [
        v({ id: "flow", label: "Flow" }),
        v({ id: "capacity", label: "Capacity used" }),
      ],
    },
  ];

  /**
   * Water distribution, whose animated list is the mixed one: three rates,
   * a categorical state (Status, pulsed for presence — conveying against
   * idle) and a carried concentration (Quality). It is the case the
   * fixtures here used to miss entirely, and the one the old wording was
   * wrong about.
   */
  const distribution = [
    { variables: [v({ id: "pressure", label: "Pressure" })] },
    {
      variables: [
        v({ id: "flow", label: "Flow" }),
        v({ id: "velocity", label: "Velocity" }),
        v({ id: "headloss", label: "Unit headloss" }),
        v({ id: "quality", label: "Quality" }),
        v({
          id: "status",
          label: "Status",
          ramp: { type: "categorical", items: [] },
        }),
      ],
    },
  ];
  const EVERY_WDS_LINK_VAR = [
    "flow",
    "velocity",
    "status",
    "headloss",
    "quality",
  ];

  it("states the rule and names what it applies to", () => {
    expect(motionExplanation(drainage, ["flow"])).toBe(
      "Motion follows the water — Flow. Anything else on this map is a still reading.",
    );
  });

  it("does not call a state a rate", () => {
    // The sentence read "Motion shows rates — …, Status, …" and then said a
    // variable measuring a state stays still: it listed one that does not.
    // Status animates because the pulse shows whether water is moving at
    // all, which is about the water and not about the status code.
    const sentence = motionExplanation(distribution, EVERY_WDS_LINK_VAR);
    expect(sentence).toContain("Status");
    expect(sentence).not.toContain("rates");
    expect(sentence).not.toContain("measures a state");
  });

  it("covers the carried case as well as the moving one", () => {
    // Quality is a concentration travelling at the water's speed; the
    // motion is honest about the water and says nothing about the number
    // the colour shows.
    expect(motionExplanation(distribution, EVERY_WDS_LINK_VAR)).toContain(
      "Quality",
    );
  });

  it("names them in the engine's own words and catalog order", () => {
    const withFlooding = [
      { variables: [v({ id: "flooding", label: "Flooding" })] },
      { variables: [v({ id: "flow", label: "Flow" })] },
    ];
    expect(motionExplanation(withFlooding, ["flow", "flooding"])).toContain(
      "Flooding and Flow",
    );
  });

  /**
   * The load-bearing omission: it must not say the rest are states. Most
   * are — depth, capacity — but a rate can be left unanimated for other
   * reasons, as wds demand is, and the sentence cannot know which is which.
   */
  it("never claims the unlisted variables are states", () => {
    const sentence = motionExplanation(drainage, ["flow"]);
    expect(sentence).not.toContain("the rest are");
    expect(sentence).not.toContain("Capacity");
    expect(sentence).not.toContain("Depth");
  });

  it("says nothing where nothing moves", () => {
    expect(motionExplanation(drainage, [])).toBe("");
  });
});

/**
 * Whether the criteria scale is on offer.
 *
 * The bug it fixes is a state mismatch rather than a wrong colour: with
 * Criteria chosen and a banded variable selected, switching to one without
 * criteria used to withdraw the option. The control fell back to Run while
 * the stored preference stayed Criteria — so it displayed one scale,
 * remembered another, and jumped back without being asked the moment a
 * banded variable was selected again.
 */
describe("criteriaScaleOffered", () => {
  const drainage = [
    { variables: [v({ id: "depth", label: "Depth" })] },
    {
      variables: [
        v({ id: "velocity", label: "Velocity" }),
        v({ id: "depth", label: "Depth" }),
      ],
    },
  ];

  it("is offered for a model that has a banded variable anywhere", () => {
    expect(criteriaScaleOffered(drainage, ["velocity"])).toBe(true);
  });

  it("stays offered while a variable without criteria is selected", () => {
    // The selection is not consulted at all — which is the fix. Depth is
    // selected in both classes here and the option remains.
    expect(criteriaScaleOffered(drainage, ["velocity"])).toBe(true);
  });

  it("is withheld from a model with no criteria at all", () => {
    // Nothing to pin a scale to: the control would present a scale that
    // does nothing for every variable, in every selection.
    expect(criteriaScaleOffered(drainage, [])).toBe(false);
    expect(criteriaScaleOffered(drainage, undefined)).toBe(false);
    expect(criteriaScaleOffered(drainage, ["pressure"])).toBe(false);
  });
});
