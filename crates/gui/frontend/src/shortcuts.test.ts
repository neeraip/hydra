import { afterEach, describe, expect, it, vi } from "vitest";
import {
  formatShortcut,
  isMacLikePlatform,
  primaryModifierLabel,
  primaryModifierPressed,
  shiftModifierLabel,
} from "./shortcuts";

function stubPlatform(platform: string) {
  vi.stubGlobal("navigator", { platform } as Navigator);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("isMacLikePlatform", () => {
  it("detects macOS / iOS platforms", () => {
    for (const p of ["MacIntel", "iPhone", "iPad"]) {
      stubPlatform(p);
      expect(isMacLikePlatform()).toBe(true);
    }
  });

  it("returns false for non-Apple platforms", () => {
    stubPlatform("Win32");
    expect(isMacLikePlatform()).toBe(false);
    stubPlatform("Linux x86_64");
    expect(isMacLikePlatform()).toBe(false);
  });

  it("returns false when navigator is unavailable", () => {
    vi.stubGlobal("navigator", undefined);
    expect(isMacLikePlatform()).toBe(false);
  });
});

describe("modifier labels + shortcut formatting", () => {
  it("uses ⌘/⇧ symbols and no separator on macOS", () => {
    stubPlatform("MacIntel");
    expect(primaryModifierLabel()).toBe("⌘");
    expect(shiftModifierLabel()).toBe("⇧");
    expect(formatShortcut(["⌘", "R"])).toBe("⌘R");
  });

  it("uses Ctrl/Shift words and a + separator elsewhere", () => {
    stubPlatform("Win32");
    expect(primaryModifierLabel()).toBe("Ctrl");
    expect(shiftModifierLabel()).toBe("Shift");
    expect(formatShortcut(["Ctrl", "R"])).toBe("Ctrl+R");
  });
});

describe("primaryModifierPressed", () => {
  it("reads metaKey on macOS and ctrlKey elsewhere", () => {
    stubPlatform("MacIntel");
    expect(primaryModifierPressed({ metaKey: true, ctrlKey: false })).toBe(
      true,
    );
    expect(primaryModifierPressed({ metaKey: false, ctrlKey: true })).toBe(
      false,
    );

    stubPlatform("Win32");
    expect(primaryModifierPressed({ metaKey: true, ctrlKey: false })).toBe(
      false,
    );
    expect(primaryModifierPressed({ metaKey: false, ctrlKey: true })).toBe(
      true,
    );
  });
});
