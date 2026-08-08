import { afterEach, describe, expect, it } from "vitest";
import { mount, unmountAll, widthOf } from "../../layoutTest";
import { SETTINGS_COLUMN, SETTINGS_HEADER } from "./SettingsDrawer";

afterEach(unmountAll);

/**
 * The drawer's column, measured in a real browser.
 *
 * This pins a bug the suite could not see: the column sized itself to its
 * contents, so it was narrow while a spinner was all it held and widened
 * as the rows arrived — carrying the header out with it. Three attempts to
 * fix that from the loading state failed, because the loading state was
 * never the cause.
 *
 * `SETTINGS_COLUMN` is imported rather than restated, so the assertion is
 * about the style the drawer actually uses.
 */

/** The panel the column lives in: a flex column, which is what makes the
 *  arrangement subtle — see `SETTINGS_COLUMN`. */
function Panel({ children }: { children: React.ReactNode }) {
  return (
    <div
      data-panel
      style={{
        width: 760,
        height: 400,
        display: "flex",
        flexDirection: "column",
        overflowY: "scroll",
      }}
    >
      <div data-column style={SETTINGS_COLUMN}>
        {children}
      </div>
    </div>
  );
}

/** The drawer as it actually nests: header, then enough rows to scroll. */
function Drawer() {
  return (
    <Panel>
      <div data-header style={SETTINGS_HEADER}>
        <h1 style={{ margin: 0 }}>Settings</h1>
      </div>
      <div style={{ height: 2000 }}>rows</div>
    </Panel>
  );
}

function boxOf(host: HTMLElement, selector: string): DOMRect {
  const el = host.querySelector(selector);
  if (!el) throw new Error(`no element matching ${selector}`);
  return el.getBoundingClientRect();
}

describe("the Settings column", () => {
  /**
   * The bug, stated directly: a narrow child and a wide one must produce
   * the same column, or whatever sits above them moves when one replaces
   * the other.
   */
  it("is the same width whether it holds a spinner or the full rows", async () => {
    const loading = widthOf(
      await mount(
        <Panel>
          <div style={{ width: 120 }}>Loading…</div>
        </Panel>,
      ),
      "[data-column]",
    );
    const loaded = widthOf(
      await mount(
        <Panel>
          <div style={{ width: 640 }}>rows</div>
        </Panel>,
      ),
      "[data-column]",
    );
    expect(loading).toBe(loaded);
  });

  /**
   * And it is the *declared* width, not merely a consistent one.
   *
   * `margin: 0 auto` on a flex item suppresses the default stretch, which
   * quietly turns `max-width: 680` into "as wide as the content, up to
   * 680". That is the exact mechanism of the bug, and the reason a
   * seemingly redundant `width: 100%` has to stay: without it this reads
   * 120, and both this test and the one above fail.
   */
  it("takes its declared width rather than its content's", async () => {
    const host = await mount(
      <Panel>
        <div style={{ width: 120 }}>Loading…</div>
      </Panel>,
    );
    expect(widthOf(host, "[data-column]")).toBe(680);
  });
});

/**
 * The header, once it was asked to stay put.
 *
 * Both claims here are geometry, so no other layer can make them: jsdom
 * answers every question about position and width with a zero, and sticky
 * is a scroll-time behaviour that only exists in a real scrollport.
 */
describe("the Settings header", () => {
  /**
   * It sits inside a column padded by 44px, so a sticky box left to itself
   * spans the text width only — and the gutters either side stay
   * transparent, showing rows sliding past while the middle hides them.
   * The negative margin is what closes that, and this is the measurement
   * that says it did.
   */
  it("is as wide as the column, not as wide as the column's text", async () => {
    const host = await mount(<Drawer />);
    expect(boxOf(host, "[data-header]").width).toBe(
      boxOf(host, "[data-column]").width,
    );
  });

  /** The point of the exercise: the way out stays reachable. */
  it("stays at the top of the panel once the rows scroll", async () => {
    const host = await mount(<Drawer />);
    const panel = host.querySelector("[data-panel]") as HTMLElement;
    const before = boxOf(host, "[data-header]").top;

    panel.scrollTop = 900;
    // A reflow, so the sticky offset is resolved before it is read.
    void panel.offsetHeight;

    const after = boxOf(host, "[data-header]").top;
    expect(panel.scrollTop).toBe(900);
    expect(after).toBe(before);
    expect(after).toBe(panel.getBoundingClientRect().top);
  });

  /** And the rows really do pass behind it rather than pushing it along. */
  it("does not travel with the content", async () => {
    const host = await mount(<Drawer />);
    const panel = host.querySelector("[data-panel]") as HTMLElement;
    const headerTop = boxOf(host, "[data-header]").top;
    panel.scrollTop = 1500;
    void panel.offsetHeight;
    expect(boxOf(host, "[data-header]").top).toBe(headerTop);
  });
});
