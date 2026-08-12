/** @vitest-environment jsdom */
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { CanvasToolbar } from "./CanvasToolbar";

/**
 * Which tools a view offers.
 *
 * The bug this pins: one flag answered two questions — "can this show a
 * basemap" and "are the positions on screen the model's own" — so a model
 * with a local grid could not move a node in any view. Its coordinates
 * were real the whole time; they simply had no georeference, which is a
 * different thing from being invented by a layout.
 */

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

function toolbar(over: Partial<React.ComponentProps<typeof CanvasToolbar>>) {
  return render(
    <CanvasToolbar
      canMoveElements
      canAddElements
      viewMode="map"
      onViewModeChange={vi.fn()}
      coordStatus="complete"
      coordMissingCount={0}
      coordTotalCount={10}
      basemap="none"
      onBasemapChange={vi.fn()}
      basemapOpacity={1}
      onBasemapOpacityChange={vi.fn()}
      showBasemapDropdown={false}
      setShowBasemapDropdown={vi.fn()}
      canvasBackground="dark"
      onCanvasBackgroundChange={vi.fn()}
      sourceCrs="EPSG:4326"
      crsError={null}
      onOpenCrsModal={vi.fn()}
      onOpenBasemapProviders={vi.fn()}
      activeTool="select"
      onToolChange={vi.fn()}
      measurePoints={[]}
      measureDistanceM={null}
      onClearAnnotations={vi.fn()}
      {...over}
    />,
  );
}

const enabled = (name: string) =>
  !(screen.getByRole("button", { name }) as HTMLButtonElement).disabled;

describe("the canvas toolbar's tools", () => {
  it("offers editing in a plan, where the coordinates are the model's own", () => {
    toolbar({ localGrid: true, viewMode: "map" });
    expect(enabled("Edit")).toBe(true);
    expect(enabled("Add node")).toBe(true);
  });

  it("offers editing on the map", () => {
    toolbar({ localGrid: false, viewMode: "map" });
    expect(enabled("Edit")).toBe(true);
    expect(enabled("Add node")).toBe(true);
  });

  it("withholds it in the schematic, whose positions are drawn", () => {
    // A node dropped there would be given a coordinate from the layout,
    // which the model never had.
    toolbar({ localGrid: false, viewMode: "schematic" });
    expect(enabled("Edit")).toBe(false);
    expect(enabled("Add node")).toBe(false);
  });

  it("says why, in terms of the view the reader is looking at", () => {
    toolbar({ localGrid: false, viewMode: "schematic" });
    expect(
      screen.getByRole("button", { name: "Edit" }).getAttribute("data-tooltip"),
    ).toContain("schematic");
  });

  it("still withholds measuring from a plan, which needs a projection", () => {
    // Distance on the map is computed through the map's own projection;
    // a local grid has none. Separate question, separate answer.
    toolbar({ localGrid: true, viewMode: "map" });
    expect(enabled("Measure distance")).toBe(false);
  });

  it("hides every editing tool for a read-only engine", () => {
    // Unchanged by the split: whether the model can be edited at all is a
    // third question, and it is the engine's to answer.
    toolbar({
      canMoveElements: false,
      canAddElements: false,
      localGrid: true,
      viewMode: "map",
    });
    expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Add node" })).toBeNull();
  });
});
