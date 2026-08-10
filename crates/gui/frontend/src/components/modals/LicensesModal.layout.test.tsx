import { afterEach, describe, expect, it } from "vitest";
import { mount, unmountAll } from "../../layoutTest";
import { MODAL_HEADER, MODAL_PANEL, TAB_STRIP } from "./LicensesModal";

afterEach(unmountAll);

/**
 * The licences panel's chrome, measured in a real browser.
 *
 * The bug this pins: on the two tabs whose page is long — the commercial
 * document and nine hundred component rows — the tab strip was squashed to
 * nothing. What was left was a panel titled "Licences" showing a document
 * with no tabs, which read as a *third* modal stacked on the second, and
 * offered no way back to the page it had been opened on.
 *
 * The mechanism is worth stating because it is invisible in the style.
 * The panel has a `max-height` and no height, so a long page forces the
 * column to shrink something; the page is `flex: 1`, whose zero basis
 * absorbs none of it, so it all lands on the chrome. `min-height: auto`
 * would have floored the strip at its content, except that floor is
 * dropped for an item whose `overflow` is not `visible` — and the strip
 * scrolls its tabs on a narrow window.
 *
 * No other layer can see this. jsdom answers every height with zero, so a
 * collapsed strip and a healthy one measure identically there.
 */

/**
 * The panel exactly as it is arranged in the app: a centring backdrop, the
 * shipped panel style with its `max-height` (not a fixed height — the
 * difference is the whole bug), and one page of the given length.
 */
function Panel({ pageHeight }: { pageHeight: number }) {
  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        display: "flex",
        alignItems: "safe center",
        justifyContent: "safe center",
        overflow: "auto",
      }}
    >
      <div data-panel style={MODAL_PANEL}>
        <div data-header style={MODAL_HEADER}>
          <h2 style={{ margin: 0 }}>Licences</h2>
        </div>
        <div data-tabs style={TAB_STRIP}>
          <button type="button" style={{ padding: "8px 12px" }}>
            Hydra's licence
          </button>
          <button type="button" style={{ padding: "8px 12px" }}>
            Commercial use
          </button>
          <button type="button" style={{ padding: "8px 12px" }}>
            Open-source components
          </button>
        </div>
        <div style={{ flex: 1, minHeight: 0, overflow: "auto" }}>
          <div style={{ height: pageHeight }}>page</div>
        </div>
      </div>
    </div>
  );
}

function heightOf(host: HTMLElement, selector: string): number {
  const el = host.querySelector(selector);
  if (!el) throw new Error(`no element matching ${selector}`);
  return el.getBoundingClientRect().height;
}

describe("the licences panel's tab strip", () => {
  it("is the same height whether its page is short or long", async () => {
    const short = heightOf(
      await mount(<Panel pageHeight={80} />),
      "[data-tabs]",
    );
    const long = heightOf(
      await mount(<Panel pageHeight={40000} />),
      "[data-tabs]",
    );
    expect(short).toBeGreaterThan(0);
    expect(long).toBe(short);
  });

  it("still shows its tabs when the page is longer than the panel", async () => {
    // The claim in the plainest form available: the way back is on screen.
    const host = await mount(<Panel pageHeight={40000} />);
    const tabs = host.querySelector("[data-tabs]") as HTMLElement;
    const first = tabs.firstElementChild as HTMLElement;
    const strip = tabs.getBoundingClientRect();
    const button = first.getBoundingClientRect();
    expect(button.height).toBeGreaterThan(0);
    expect(button.bottom).toBeLessThanOrEqual(strip.bottom + 1);
  });

  it("keeps the header too, which is where the way out lives", async () => {
    const host = await mount(<Panel pageHeight={40000} />);
    expect(heightOf(host, "[data-header]")).toBeGreaterThan(0);
  });
});
