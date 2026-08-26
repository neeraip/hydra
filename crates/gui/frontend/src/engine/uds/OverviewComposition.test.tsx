/** @vitest-environment jsdom */
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

/**
 * What the Overview says about a drainage model.
 *
 * The gap this closes: a model carrying a 2D surface looked exactly like
 * one without, everywhere, until a run had produced results for the
 * canvas to colour. The mesh is a property of the *model*, so the
 * Overview states it from import — including for a model nobody intends
 * to run.
 */

const meshInfo = vi.fn();

vi.mock("../../hooks", () => ({
  useNodes: () => [
    { id: "J1", type: "junction" },
    { id: "O1", type: "outfall" },
  ],
  useLinks: () => [{ id: "C1", type: "conduit" }],
  useRegions: () => [],
}));

vi.mock("../../hooks/surface", () => ({
  useMeshInfo: (loaded: boolean) => meshInfo(loaded),
}));

const { UdsOverviewComposition } = await import("./OverviewComposition");

beforeEach(() => {
  meshInfo.mockReset();
});

function composition() {
  return render(
    <UdsOverviewComposition
      networkLoaded
      fallbackNodeCount={2}
      fallbackLinkCount={1}
    />,
  );
}

describe("UdsOverviewComposition", () => {
  it("states the surface a mesh model carries, before any run", () => {
    meshInfo.mockReturnValue({ nVertices: 9, nCells: 8 });
    composition();
    expect(screen.getByText("Surface cells")).toBeTruthy();
    expect(screen.getByText("8")).toBeTruthy();
    expect(screen.getByText(/9 vertices/)).toBeTruthy();
  });

  it("says nothing about a surface for a model without one", () => {
    meshInfo.mockReturnValue(null);
    composition();
    expect(screen.queryByText("Surface cells")).toBeNull();
    // The rest of the grid is unaffected.
    expect(screen.getByText("Nodes")).toBeTruthy();
    expect(screen.getByText("Outfalls")).toBeTruthy();
  });

  it("asks only once the network is loaded", () => {
    meshInfo.mockReturnValue(null);
    render(
      <UdsOverviewComposition
        networkLoaded={false}
        fallbackNodeCount={2}
        fallbackLinkCount={1}
      />,
    );
    expect(meshInfo).toHaveBeenCalledWith(false);
  });
});
