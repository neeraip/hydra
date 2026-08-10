/** @vitest-environment jsdom */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { beforeEach, describe, expect, it } from "vitest";
import {
  KEPT_KEYS,
  KEPT_PREFIXES,
  resetPreferences,
  SETTINGS_KEYS,
  VIEW_KEYS,
} from "./preferences";

/**
 * The reset is only as good as its list, and the list is hand-written.
 *
 * So the list is checked against the source rather than against itself: a
 * preference added beside the feature it governs — which is where it
 * belongs — would otherwise survive a reset silently, and nobody would
 * find out by reading this file.
 */

const SRC = join(import.meta.dirname ?? __dirname);

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) {
      out.push(...sourceFiles(path));
      // Tests are skipped: they set keys to exercise the app's own, and
      // this file's prose mentions the prefix, both of which would read as
      // new keys.
    } else if (
      /\.tsx?$/.test(name) &&
      !/\.test\.tsx?$|^preferences\.ts$/.test(name)
    ) {
      out.push(path);
    }
  }
  return out;
}

/** Every `hydra2-…` storage key written anywhere in the app. */
function keysInSource(): Set<string> {
  const found = new Set<string>();
  for (const file of sourceFiles(SRC)) {
    const text = readFileSync(file, "utf8");
    for (const match of text.matchAll(/["'`](hydra2-[a-z0-9-]*:?)/g)) {
      found.add(match[1]);
    }
  }
  return found;
}

describe("the preference inventory", () => {
  it("classifies every storage key the app writes", () => {
    const classified = new Set<string>([
      ...SETTINGS_KEYS,
      ...VIEW_KEYS,
      ...KEPT_KEYS,
    ]);
    const unclassified = [...keysInSource()].filter(
      (key) =>
        !classified.has(key) && !KEPT_PREFIXES.some((p) => key.startsWith(p)),
    );
    expect(
      unclassified,
      "a new hydra2- key must be added to SETTINGS_KEYS, VIEW_KEYS or " +
        "KEPT_KEYS — otherwise it silently survives a reset, or is silently " +
        "destroyed by one",
    ).toEqual([]);
  });

  it("keeps the two lists disjoint", () => {
    // A key in both would be reset and claimed to be kept.
    const reset = new Set<string>([...SETTINGS_KEYS, ...VIEW_KEYS]);
    expect(KEPT_KEYS.filter((k) => reset.has(k))).toEqual([]);
  });
});

describe("resetPreferences", () => {
  beforeEach(() => localStorage.clear());

  it("removes the preferences and counts what was there", () => {
    localStorage.setItem("hydra2-theme", "light");
    localStorage.setItem("hydra2-rail-width", "420");
    expect(resetPreferences()).toBe(2);
    expect(localStorage.getItem("hydra2-theme")).toBeNull();
    expect(localStorage.getItem("hydra2-rail-width")).toBeNull();
  });

  it("leaves work and bookkeeping alone", () => {
    // Resetting how the app looks must not lose which project you were in
    // or replay release notes you have already read.
    localStorage.setItem("hydra2-last-project", "p1");
    localStorage.setItem("hydra2-last-seen-gui-version", "2.14.0");
    localStorage.setItem("hydra2-canvas-prefs:p1", "{}");
    resetPreferences();
    expect(localStorage.getItem("hydra2-last-project")).toBe("p1");
    expect(localStorage.getItem("hydra2-last-seen-gui-version")).toBe("2.14.0");
    expect(localStorage.getItem("hydra2-canvas-prefs:p1")).toBe("{}");
  });

  it("reports nothing when there was nothing to reset", () => {
    expect(resetPreferences()).toBe(0);
  });
});
