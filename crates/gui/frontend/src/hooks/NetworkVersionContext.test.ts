/**
 * Tests for the microtask coalescing behind `bumpNetwork`
 * (makeCoalescedScheduler): N synchronous bumps in one tick must produce a
 * single version increment; bumps in separate macrotasks increment
 * separately.
 */
import { describe, expect, it, vi } from "vitest";

// NetworkVersionContext imports the hooks barrel for listenNetworkChanged;
// stub it so this pure-logic test doesn't pull the Tauri event plumbing.
vi.mock("./index", () => ({
  listenNetworkChanged: vi.fn(async () => () => {}),
}));

import { makeCoalescedScheduler } from "./NetworkVersionContext";

const flushMicrotasks = () => Promise.resolve();
const nextMacrotask = () =>
  new Promise<void>((resolve) => setTimeout(resolve, 0));

describe("makeCoalescedScheduler", () => {
  it("coalesces N synchronous calls into one invocation after microtask flush", async () => {
    const fn = vi.fn();
    const bump = makeCoalescedScheduler(fn);

    bump();
    bump();
    bump();
    bump();
    bump();
    expect(fn).not.toHaveBeenCalled(); // nothing runs synchronously

    await flushMicrotasks();
    expect(fn).toHaveBeenCalledTimes(1);

    // No stray trailing invocations later.
    await nextMacrotask();
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("invokes once per macrotask batch", async () => {
    const fn = vi.fn();
    const bump = makeCoalescedScheduler(fn);

    bump();
    bump();
    await nextMacrotask();
    expect(fn).toHaveBeenCalledTimes(1);

    bump();
    bump();
    bump();
    await nextMacrotask();
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("a call made after the flush schedules a fresh invocation", async () => {
    const fn = vi.fn();
    const bump = makeCoalescedScheduler(fn);

    bump();
    await flushMicrotasks();
    expect(fn).toHaveBeenCalledTimes(1);

    bump();
    await flushMicrotasks();
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("independent schedulers do not share pending state", async () => {
    const a = vi.fn();
    const b = vi.fn();
    const bumpA = makeCoalescedScheduler(a);
    const bumpB = makeCoalescedScheduler(b);

    bumpA();
    bumpB();
    bumpA();
    await flushMicrotasks();
    expect(a).toHaveBeenCalledTimes(1);
    expect(b).toHaveBeenCalledTimes(1);
  });
});
