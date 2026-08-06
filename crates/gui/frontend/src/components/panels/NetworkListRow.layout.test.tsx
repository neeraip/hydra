import { afterEach, describe, expect, it } from "vitest";
import { mount, unmountAll, widthOf } from "../../layoutTest";
import { NetworkListRow, type Row } from "./NetworkListRow";

afterEach(unmountAll);

/**
 * A row asked how wide it wants to be.
 *
 * This is what fitting the panel to its contents rests on: the measurer
 * renders one real row with the widest content and reads its box. If the
 * row reports its container's width back instead of its own, the fit is
 * a no-op that looks like it works — the panel would resize to whatever
 * it already was.
 *
 * jsdom cannot answer any of this; it reports every width as zero.
 */

const NOOP = () => {};

const row = (over: Partial<Row>): Row => ({
  id: "J1",
  kind: "junction",
  cls: "point",
  context: "",
  value: null,
  format: null,
  canZoom: false,
  ...over,
});

const HEADING = {
  text: "Flow",
  tip: "Flow",
  perRowUnits: false,
  unitWidth: 0,
};

async function widthOfRow(r: Row, intrinsic: boolean, container = 900) {
  const host = await mount(
    <div data-wrap style={{ width: container }}>
      <NetworkListRow
        intrinsic={intrinsic}
        row={r}
        isActive={false}
        zoomable={r.canZoom}
        searching={r.context.length > 0}
        sys="si"
        valueHeading={HEADING}
        kindLabel={new Map()}
        onSelect={NOOP}
        onZoom={NOOP}
        onHover={NOOP}
        onClearHover={NOOP}
      />
    </div>,
  );
  return widthOf(host, "[data-wrap] button");
}

describe("a network list row", () => {
  /** How it renders in the list: fills whatever the panel gives it. */
  it("fills its container normally", async () => {
    expect(await widthOfRow(row({ id: "J1" }), false, 900)).toBe(900);
    expect(await widthOfRow(row({ id: "J1" }), false, 400)).toBe(400);
  });

  /**
   * The measuring mode, and the reason it exists: the width has to come
   * from the row's contents, not from the space it was offered.
   */
  it("reports its own width when measured", async () => {
    const wide = await widthOfRow(row({ id: "J1" }), true, 900);
    const narrow = await widthOfRow(row({ id: "J1" }), true, 400);
    expect(wide).toBe(narrow);
    expect(wide).toBeLessThan(400);
  });

  /** And that width tracks the content, or fitting would be pointless. */
  it("grows with a longer id", async () => {
    const short = await widthOfRow(row({ id: "J1" }), true);
    const long = await widthOfRow(row({ id: "JUNCTION-000123-A" }), true);
    expect(long).toBeGreaterThan(short);
  });

  /**
   * The subtitle is the row's second line, so it widens the row without
   * lengthening the first — a fit that only looked at ids would clip it.
   */
  it("grows for a subtitle longer than the id", async () => {
    const plain = await widthOfRow(row({ id: "J1" }), true);
    const withContext = await widthOfRow(
      row({ id: "J1", context: "J-100 → J-200" }),
      true,
    );
    expect(withContext).toBeGreaterThan(plain);
  });

  /**
   * The zoom affordance widens a row's padding, which is why the fit
   * tracks whether any row has one.
   */
  it("is wider when it carries the zoom affordance", async () => {
    const plain = await widthOfRow(row({ id: "J1" }), true);
    const zoomable = await widthOfRow(row({ id: "J1", canZoom: true }), true);
    expect(zoomable).toBeGreaterThan(plain);
  });
});
