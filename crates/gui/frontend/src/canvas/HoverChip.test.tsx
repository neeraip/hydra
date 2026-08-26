/** @vitest-environment jsdom */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { PeriodResults } from "../hooks";
import { HoverChip, type HoverTip } from "./HoverChip";
import type { GenericCanvasResults } from "./types";

/**
 * The chip that follows the pointer across the canvas.
 *
 * It answered for nodes and links and said nothing at all over a
 * subcatchment — hovering an areal element highlighted it and reported no
 * id and no value, on the one class where the outline alone tells you
 * least about which catchment you are on.
 *
 * The second thing tested here is why that could not simply be switched
 * on: `si` is a position in its own class's array, and the three arrays
 * are different sequences. A region index read against the link channel
 * does not fail — it prints another element's flow.
 */

const sys = "si" as const;

function channel(values: number[], key: string) {
  return {
    variable: {
      id: key,
      label: key,
      quantity: {
        key,
        siLabel: "m³/s",
        usLabel: "cfs",
        siToUsScale: 35.3147,
        siToUsOffset: 0,
        siDecimals: 3,
        usDecimals: 2,
      },
      ramp: { kind: "linear" } as never,
      min: 0,
      max: 10,
    },
    values: Float32Array.from(values),
  };
}

const generic: GenericCanvasResults = {
  node: channel([1, 2, 3], "node_q"),
  link: channel([70, 80, 90], "link_q"),
  region: channel([0.5, 0.25, 0.125], "runoff"),
};

function tip(over: Partial<HoverTip> = {}): HoverTip {
  return {
    x: 10,
    y: 20,
    kind: "region",
    type: "subcatchment",
    si: 1,
    id: "S2",
    ...over,
  };
}

function chip(props: Partial<React.ComponentProps<typeof HoverChip>> = {}) {
  return render(
    <HoverChip
      tip={tip()}
      periodResult={null}
      generic={generic}
      nodeVar="pressure"
      linkVar="flow"
      sys={sys}
      {...props}
    />,
  );
}

describe("HoverChip over an areal element", () => {
  it("names the catchment and reads its own channel", () => {
    chip();
    expect(screen.getByText("S2")).toBeTruthy();
    // 0.25 is the region channel's second value. The link channel's second
    // value is 80 — a number that would look perfectly plausible here.
    expect(screen.getByText(/0\.25/)).toBeTruthy();
    expect(screen.queryByText(/80/)).toBeNull();
  });

  it("still names it when the run reported nothing for that class", () => {
    // A model with catchments but no runoff results: the id is the whole
    // point of the chip, and withholding it because there is no value
    // would leave the hover doing nothing again.
    chip({ generic: { ...generic, region: null } });
    expect(screen.getByText("S2")).toBeTruthy();
  });

  it("reads no value at all from water-distribution results", () => {
    // Those channels have no areal class. The `else` branch inside is the
    // link one, so an unguarded region index would report a pipe's flow as
    // the catchment's.
    const periodResult = {
      linkFlow: Float32Array.from([7, 8, 9]),
    } as unknown as PeriodResults;
    const { container } = chip({ generic: null, periodResult });
    expect(screen.getByText("S2")).toBeTruthy();
    expect(container.textContent).not.toContain("8");
  });
});

describe("HoverChip over a surface cell", () => {
  const surface = {
    data: {
      length: 3,
      startIndices: new Uint32Array([0, 3, 6]),
      attributes: {
        getPolygon: { value: new Float64Array(18), size: 2 as const },
      },
    },
    colors: new Uint8Array(36),
    variable: channel([], "depth").variable,
    values: Float32Array.from([0.1, 1.375, 0.4]),
  };

  // A cell is not an element: no id, no badge — the chip names it by
  // index and reads the on-show surface variable, converted like every
  // generic value.
  it("names the cell and reads the surface channel", () => {
    chip({
      tip: tip({ kind: "surface", type: "surface", si: 1, id: "Cell 1" }),
      surface,
    });
    expect(screen.getByText("Cell 1")).toBeTruthy();
    expect(screen.getByText(/1\.375/)).toBeTruthy();
  });

  // The surface channel is its own sequence; a cell index must never be
  // read against an element channel, and element tips must never read
  // the surface values.
  it("does not read element channels for a cell, nor cells for elements", () => {
    const first = chip({
      tip: tip({ kind: "surface", type: "surface", si: 1, id: "Cell 1" }),
      // generic present and would answer 80 for si=1 on the link channel.
      surface,
    });
    expect(screen.queryByText(/80/)).toBeNull();
    first.unmount();
    chip({
      tip: tip({ kind: "link", type: "conduit", si: 1, id: "C2" }),
      surface,
    });
    expect(screen.queryByText(/1\.375/)).toBeNull();
  });

  it("still names the cell when the surface channel is absent", () => {
    chip({
      tip: tip({ kind: "surface", type: "surface", si: 0, id: "Cell 0" }),
      surface: null,
    });
    expect(screen.getByText("Cell 0")).toBeTruthy();
  });
});

describe("HoverChip over the other classes", () => {
  it("still reads each class against its own channel", () => {
    chip({ tip: tip({ kind: "node", type: "junction", id: "J1", si: 0 }) });
    expect(screen.getByText("J1")).toBeTruthy();
    expect(screen.getByText(/1\.0/)).toBeTruthy();
  });

  it("renders nothing when nothing is hovered", () => {
    const { container } = chip({ tip: null });
    expect(container.firstChild).toBeNull();
  });
});
