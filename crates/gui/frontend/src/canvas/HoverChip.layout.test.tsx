import { afterEach, describe, expect, it } from "vitest";
import { mount, unmountAll } from "../layoutTest";
import { HoverChip, type HoverTip } from "./HoverChip";

/**
 * The chip is one line, whatever leads it.
 *
 * The defect this pins: the preflight reset makes every `svg`
 * block-level, so the surface cell's triangle glyph took the chip's
 * first line for itself and the text dropped to a second — a cascade
 * product invisible to jsdom, which answers every box question with
 * zero. Asked of a real browser, with app.css loaded, the way the
 * layout project exists to ask.
 */

afterEach(() => {
  unmountAll();
});

function tip(over: Partial<HoverTip>): HoverTip {
  return {
    x: 10,
    y: 20,
    kind: "node",
    type: "junction",
    si: 0,
    id: "J1",
    ...over,
  };
}

const surface = {
  key: "test",
  geometry: {
    nVertices: 3,
    nCells: 1,
    positions: new Float64Array(9),
    triangles: new Uint32Array([0, 1, 2]),
  },
  data: {
    length: 1,
    startIndices: new Uint32Array([0]),
    attributes: {
      getPolygon: { value: new Float64Array(6), size: 2 as const },
    },
    bounds: null,
  },
  edges: {
    length: 0,
    attributes: {
      getSourcePosition: { value: new Float64Array(0), size: 2 as const },
      getTargetPosition: { value: new Float64Array(0), size: 2 as const },
    },
    medianLength: 0,
  },
  colors: new Uint8Array(12),
  cellColors: new Uint8Array(12),
  variable: {
    id: "depth",
    label: "Depth",
    ramp: { type: "sequential" } as const,
    min: 0,
    max: 2,
  },
  vertexValues: null,
  centreValues: null,
  corners: null,
  bary: new Float32Array(9),
  values: Float32Array.from([0.017]),
};

async function chipHeight(t: HoverTip): Promise<number> {
  const host = await mount(
    <HoverChip
      tip={t}
      periodResult={null}
      generic={null}
      surface={surface}
      nodeVar="pressure"
      linkVar="flow"
      sys="si"
    />,
  );
  const chip = host.firstElementChild as HTMLElement;
  expect(chip).toBeTruthy();
  return chip.getBoundingClientRect().height;
}

describe("HoverChip glyph alignment", () => {
  it("a surface cell's chip is as tall as an element's, one line", async () => {
    const element = await chipHeight(tip({}));
    const cell = await chipHeight(
      tip({ kind: "surface", type: "surface", id: "Cell 0" }),
    );
    // The glyph must ride the text line, not stack above it. The badge
    // chip is the taller single-line reference (the badge outsizes the
    // text), so a glyph chip that exceeds it has taken a second line —
    // a stacked 10px glyph lands ~10px over, which a loose ratio bound
    // waved through when this test was first written.
    expect(element).toBeGreaterThan(0);
    expect(cell).toBeLessThanOrEqual(element + 2);
  });
});
