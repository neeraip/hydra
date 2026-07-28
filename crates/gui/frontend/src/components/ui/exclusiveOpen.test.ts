import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  claimExclusive,
  hasExclusiveHolder,
  releaseExclusive,
} from "./exclusiveOpen";

describe("exclusiveOpen", () => {
  beforeEach(() => {
    // Drain any holder left by a previous test.
    const drain = () => {};
    claimExclusive(drain);
    releaseExclusive(drain);
  });

  it("closes the previous holder when a new one claims", () => {
    const closeA = vi.fn();
    const closeB = vi.fn();

    claimExclusive(closeA);
    expect(closeA).not.toHaveBeenCalled();

    claimExclusive(closeB);
    expect(closeA).toHaveBeenCalledTimes(1);
    expect(closeB).not.toHaveBeenCalled();
  });

  it("does not close the holder when it re-claims", () => {
    // A menu toggling itself must not be told to close by its own claim —
    // that would fight the component's own state.
    const close = vi.fn();
    claimExclusive(close);
    claimExclusive(close);
    expect(close).not.toHaveBeenCalled();
  });

  it("releases only for the current holder", () => {
    const closeA = vi.fn();
    const closeB = vi.fn();

    claimExclusive(closeA);
    claimExclusive(closeB);

    // A closes/unmounts after being superseded. It must not clear B's slot,
    // or the next claim would leave B open alongside the newcomer.
    releaseExclusive(closeA);
    expect(hasExclusiveHolder()).toBe(true);

    const closeC = vi.fn();
    claimExclusive(closeC);
    expect(closeB).toHaveBeenCalledTimes(1);
  });

  it("frees the slot when the current holder releases", () => {
    const close = vi.fn();
    claimExclusive(close);
    releaseExclusive(close);
    expect(hasExclusiveHolder()).toBe(false);

    // With the slot free, the next claim has nobody to close.
    const next = vi.fn();
    claimExclusive(next);
    expect(close).not.toHaveBeenCalled();
    expect(next).not.toHaveBeenCalled();
  });

  it("never calls the incoming holder's own callback", () => {
    const closeA = vi.fn();
    const closeB = vi.fn();
    claimExclusive(closeA);
    claimExclusive(closeB);
    claimExclusive(closeA);
    expect(closeA).toHaveBeenCalledTimes(1);
    expect(closeB).toHaveBeenCalledTimes(1);
  });
});
