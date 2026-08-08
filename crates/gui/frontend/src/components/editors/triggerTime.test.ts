import { describe, expect, it } from "vitest";
import {
  parseTriggerTime,
  triggerTimeIsElapsed,
  triggerTimeText,
} from "./triggerTime";

const H = 3600;

describe("an elapsed trigger time", () => {
  /**
   * The defect. One formatter served both kinds and it wrapped at a day, so
   * a control imported as `AT TIME 30` showed six o'clock — the same as a
   * genuine six-hour control — and committing the field wrote six hours
   * back. Two of these three assertions passed before; the middle one is
   * the whole bug.
   */
  it("shows hours past a day as hours past a day", () => {
    expect(triggerTimeText("timer", 6 * H)).toBe("06:00");
    expect(triggerTimeText("timer", 30 * H)).toBe("30:00");
    expect(triggerTimeText("timer", 49 * H)).toBe("49:00");
  });

  it("tells two controls a day apart apart", () => {
    expect(triggerTimeText("timer", 6 * H)).not.toBe(
      triggerTimeText("timer", 30 * H),
    );
  });

  /** The half that did the damage: reading the field back after showing it. */
  it("round-trips whatever it showed", () => {
    for (const seconds of [0, 90, 6 * H, 30 * H, 49 * H, 100 * H + 61]) {
      expect(parseTriggerTime("timer", triggerTimeText("timer", seconds))).toBe(
        seconds,
      );
    }
  });

  /** Seconds used to vanish on the way out and never came back. */
  it("keeps seconds when a control has them", () => {
    expect(triggerTimeText("timer", 90)).toBe("00:01:30");
    expect(parseTriggerTime("timer", "00:01:30")).toBe(90);
  });

  it("takes a text field, since a time picker cannot hold 30:00", () => {
    expect(triggerTimeIsElapsed("timer")).toBe(true);
  });
});

describe("a clock trigger time", () => {
  /** Here the wrap is right: it is a reading on a wall clock. */
  it("wraps at midnight", () => {
    expect(triggerTimeText("clocktime", 6 * H)).toBe("06:00");
    expect(triggerTimeText("clocktime", 30 * H)).toBe("06:00");
    expect(parseTriggerTime("clocktime", "30:00")).toBe(6 * H);
  });

  it("round-trips a time of day", () => {
    for (const seconds of [0, 60, 6 * H, 23 * H + 59 * 60]) {
      expect(
        parseTriggerTime("clocktime", triggerTimeText("clocktime", seconds)),
      ).toBe(seconds);
    }
  });

  /**
   * The one thing that does not round-trip, and deliberately. The native
   * picker is minute-resolution, and a value carrying seconds renders as an
   * empty field rather than as an approximation.
   */
  it("is given to the minute, because its control is", () => {
    expect(triggerTimeText("clocktime", 6 * H + 30)).toBe("06:01");
  });

  it("keeps the picker", () => {
    expect(triggerTimeIsElapsed("clocktime")).toBe(false);
  });
});

describe("a field that does not hold a time", () => {
  /**
   * Null, not zero. The old parser read an empty field as zero, so clearing
   * the input moved the control to the start of the run — and a time input
   * reports empty on the way through most edits.
   */
  it("reads as nothing rather than as the start of the run", () => {
    for (const text of [
      "",
      "  ",
      ":",
      "abc",
      "12:",
      ":30",
      "12:60",
      "1:2:99",
    ]) {
      expect(parseTriggerTime("timer", text), text).toBeNull();
    }
  });

  it("still reads a time that is merely untidy", () => {
    expect(parseTriggerTime("timer", " 6:5 ")).toBe(6 * H + 5 * 60);
  });

  it("shows a missing time as the start of the run", () => {
    expect(triggerTimeText("timer", null)).toBe("00:00");
    expect(triggerTimeText("timer", Number.NaN)).toBe("00:00");
    expect(triggerTimeText("timer", -5)).toBe("00:00");
  });
});
