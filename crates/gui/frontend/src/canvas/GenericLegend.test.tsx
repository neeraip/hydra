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
