import { afterEach, describe, expect, it, vi } from "vitest";
import { whenIdle } from "./lazyChunks";

const realRIC = globalThis.requestIdleCallback;
const realCIC = globalThis.cancelIdleCallback;

afterEach(() => {
  globalThis.requestIdleCallback = realRIC;
  globalThis.cancelIdleCallback = realCIC;
  vi.useRealTimers();
});

describe("whenIdle", () => {
  it("uses the idle callback when the engine has one", () => {
    const ric = vi.fn().mockReturnValue(7);
    const cic = vi.fn();
    globalThis.requestIdleCallback = ric as never;
    globalThis.cancelIdleCallback = cic as never;

    const cancel = whenIdle(() => {});
    expect(ric).toHaveBeenCalledOnce();
    // Bounded, so a page that never goes idle still prefetches.
    expect(ric.mock.calls[0][1]).toMatchObject({ timeout: expect.any(Number) });
    cancel();
    expect(cic).toHaveBeenCalledWith(7);
  });

  /**
   * WebKit has no `requestIdleCallback`, and WebKit is the engine the macOS
   * build runs on — so the platform most likely to lack it is one we ship
   * to. A bare call would silently never prefetch there, which is exactly
   * the case this exists to fix.
   */
  it("still runs when the engine has no idle callback", () => {
    vi.useFakeTimers();
    // @ts-expect-error — modelling an engine that does not provide it.
    globalThis.requestIdleCallback = undefined;
    const fn = vi.fn();

    whenIdle(fn);
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1000);
    expect(fn).toHaveBeenCalledOnce();
  });

  it("can be cancelled on that path too", () => {
    vi.useFakeTimers();
    // @ts-expect-error — modelling an engine that does not provide it.
    globalThis.requestIdleCallback = undefined;
    const fn = vi.fn();

    whenIdle(fn)();
    vi.advanceTimersByTime(5000);
    expect(fn).not.toHaveBeenCalled();
  });
});
