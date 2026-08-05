import { afterEach, describe, expect, it } from "vitest";
import { mount, unmountAll, widthOf } from "../../layoutTest";
import { SETTINGS_COLUMN } from "./SettingsDrawer";

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
