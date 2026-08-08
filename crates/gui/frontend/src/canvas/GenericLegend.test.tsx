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
import { GenericLegend } from "./GenericLegend";

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
    expect(screen.queryByText("Criteria")).toBeNull();
    expect(screen.getByText("Whole run")).toBeDefined();
    expect(screen.getByText("This step")).toBeDefined();
  });

  it("offers Criteria when the selected variable has bands", () => {
    renderLegend({
      criteriaVariables: ["flow"],
      selection: { point: "", polyline: "flow", region: "" },
    });
    expect(screen.getByText("Criteria")).toBeDefined();
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
    fireEvent.click(screen.getByText("This step"));
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
