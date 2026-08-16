import { describe, expect, it } from "vitest";
import { fetchInto } from "./fetchInto";

/**
 * The one behaviour all forty-odd hand-rolled copies existed for: a slow
 * answer must not overwrite a newer question's. Everything else about
 * the idiom — when it runs, what resets — stayed at the call sites.
 */
describe("fetchInto", () => {
  it("applies the result when nothing cancelled it", async () => {
    let got: string | null = null;
    fetchInto(Promise.resolve("value"), (v) => {
      got = v;
    });
    await Promise.resolve();
    expect(got).toBe("value");
  });

  it("drops the result once cleaned up", async () => {
    // The stale-answer race: the effect re-ran (cleanup fired) while the
    // old fetch was still in flight. Its answer describes a question
    // nobody is asking any more.
    let got: string | null = null;
    const cancel = fetchInto(Promise.resolve("stale"), (v) => {
      got = v;
    });
    cancel();
    await Promise.resolve();
    expect(got).toBeNull();
  });

  it("keeps two overlapping fetches from crossing", async () => {
    // What an effect re-run actually does: cancel the old, start the
    // new. Only the new one's answer lands, whatever order they resolve.
    let shown: string | null = null;
    let resolveSlow: (v: string) => void = () => {};
    const slow = new Promise<string>((r) => {
      resolveSlow = r;
    });
    const cancelSlow = fetchInto(slow, (v) => {
      shown = v;
    });

    cancelSlow();
    fetchInto(Promise.resolve("current"), (v) => {
      shown = v;
    });
    await Promise.resolve();
    resolveSlow("stale");
    await Promise.resolve();
    expect(shown).toBe("current");
  });
});
