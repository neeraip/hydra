import { afterEach, describe, expect, it } from "vitest";
import { mount, unmountAll, widthOf } from "../layoutTest";
import { Timeline } from "./Timeline";

afterEach(unmountAll);

/**
 * The scrubber's width, measured in a real browser.
 *
 * This is the invariant the user actually feels: the track takes whatever
 * the readout beside it leaves, so a readout that grows with its own
 * content narrows the track, and every tick and the playhead shift under a
 * stationary cursor. Stepping 9 → 10 did that, and stepping back undid it.
 *
 * jsdom cannot see any of this — it reports every width as zero, so the
 * readout and the track measure the same before and after the fix.
 */

const NOOP = () => {};

/**
 * The largest width change that is not a visible jump, in pixels.
 *
 * Not zero, because `--font-mono` does not resolve to a perfectly
 * monospaced face in every browser: a reservation stated in `ch` — the
 * advance of "0" — lands a fraction of a pixel from a string whose letters
 * are not all digits. The residue measures under a tenth of a pixel.
 *
 * Half a pixel is therefore comfortably above the noise and far below the
 * defect, which moved the track by a whole character — about 7px at this
 * text size, two orders of magnitude clear of this bound.
 */
const NO_VISIBLE_JUMP = 0.5;

/** Assert two measured widths differ by less than a visible amount. */
function expectSameWidth(actual: number, expected: number) {
  expect(Math.abs(actual - expected)).toBeLessThan(NO_VISIBLE_JUMP);
}

function Bar({
  currentHour,
  maxStep,
}: {
  currentHour: number;
  maxStep: number;
}) {
  return (
    <div data-wrap style={{ width: 900 }}>
      <Timeline
        currentHour={currentHour}
        setCurrentHour={NOOP}
        isPlaying={false}
        setIsPlaying={NOOP}
        speed={1}
        setSpeed={NOOP}
        loop={false}
        setLoop={NOOP}
        maxStep={maxStep}
      />
    </div>
  );
}

async function trackWidth(currentHour: number, maxStep = 24): Promise<number> {
  const host = await mount(<Bar currentHour={currentHour} maxStep={maxStep} />);
  return widthOf(host, '[role="slider"]');
}

describe("the timeline's scrubber", () => {
  /**
   * The bug, stated directly. Period 10 is one character wider than period
   * 9 in the readout; the track must not pay for it.
   */
  it("keeps its width as the period counter gains a digit", async () => {
    expectSameWidth(await trackWidth(9), await trackWidth(8));
  });

  /**
   * And across the whole run rather than at one boundary — the first
   * period, the tens boundary, and the last must all agree.
   */
  it("keeps its width from the first period to the last", async () => {
    const first = await trackWidth(0);
    expectSameWidth(await trackWidth(9), first);
    expectSameWidth(await trackWidth(24), first);
  });

  /**
   * A longer run reserves more room for its counter, so its track is
   * legitimately narrower — but still constant within itself. This is what
   * distinguishes the fix from simply hard-coding a width: the reservation
   * tracks the run, and only the *current value* is prevented from moving
   * it.
   */
  it("stays constant within a run that reaches three digits", async () => {
    const early = await trackWidth(5, 120);
    expectSameWidth(await trackWidth(99, 120), early);
    expectSameWidth(await trackWidth(120, 120), early);
  });
});
