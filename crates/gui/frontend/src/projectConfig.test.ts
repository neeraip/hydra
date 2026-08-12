import { describe, expect, it } from "vitest";
import {
  engineByKey,
  FALLBACK_ENGINES,
  importExtensionLabel,
  isEngineGuiOpenable,
} from "./hooks/engines";
import type { ProjectView } from "./projectConfig";
import { ACCENT, PROJECT_VIEWS } from "./projectConfig";

// ── App accent + engine registry fallback ────────────────────────────────────

describe("app accent", () => {
  it("ACCENT is the theme token, not a copy of its value", () => {
    // A literal here cannot follow the theme, and the literal it held was
    // the wds engine's own colour — the collision the accent change removed.
    expect(ACCENT).toBe("var(--accent)");
  });
});

describe("engine registry fallback", () => {
  it("carries the wds engine with a 2-char pill and hex accent", () => {
    const wds = engineByKey(FALLBACK_ENGINES, "wds");
    expect(wds).not.toBeNull();
    expect(wds?.label.length).toBeGreaterThan(0);
    expect(wds?.pill).toBe("WD");
    expect(wds?.accent).toMatch(/^#[0-9a-fA-F]{6}$/);
  });

  it("unknown keys resolve to null, never a default engine", () => {
    expect(engineByKey(FALLBACK_ENGINES, "nope")).toBeNull();
    expect(engineByKey(FALLBACK_ENGINES, "")).toBeNull();
  });

  it("registers an engine this GUI cannot open, so the wizard can present it", () => {
    // Unopenable ≠ unknown (hydra-common spec §2.3): a planned engine
    // resolves and carries full identity, so its card can be drawn and
    // disabled, but it must never back a project here.
    const engine = engineByKey(FALLBACK_ENGINES, "och");
    if (engine === null) throw new Error("och must be registered");
    expect(engine.pill).toHaveLength(2);
    expect(isEngineGuiOpenable(engine)).toBe(false);
  });

  it("opens a project for every implemented engine", () => {
    // This asserted "only wds can back a project", which was true while
    // drainage was a viewer and stayed on the page after it stopped
    // being. What it is really guarding is that a *planned* engine
    // cannot — so it names that, and lets the implemented list grow.
    const openable = FALLBACK_ENGINES.filter(isEngineGuiOpenable).map(
      (e) => e.key,
    );
    expect(openable).toEqual(["wds", "uds"]);
  });

  it("mirrors the backend registry order", () => {
    // The fallback stands in for `list_engines` outside a Tauri shell — a
    // divergence here would make the wizard's card order depend on how the
    // app was launched.
    expect(FALLBACK_ENGINES.map((e) => e.key)).toEqual(["wds", "uds", "och"]);
  });

  it("every engine declares importable formats", () => {
    for (const engine of FALLBACK_ENGINES) {
      expect(engine.import.length).toBeGreaterThan(0);
      for (const format of engine.import) {
        expect(format.label.length).toBeGreaterThan(0);
        expect(format.extensions.length).toBeGreaterThan(0);
        // A leading dot or uppercase would break the picker filter.
        for (const ext of format.extensions) {
          expect(ext).toMatch(/^[a-z0-9]+$/);
        }
      }
    }
  });

  it("renders extension hints with dots and no duplicates", () => {
    expect(importExtensionLabel(FALLBACK_ENGINES[0])).toBe(".inp");
    const och = engineByKey(FALLBACK_ENGINES, "och");
    if (och === null) throw new Error("och must be registered");
    expect(importExtensionLabel(och)).toBe(".zip, .7z, .tar, .gz, .tgz");
  });

  it("wds and uds share the .inp extension", () => {
    // The reason the extension can never stand in for validation: only the
    // engine's own parser can tell an EPANET model from a SWMM one.
    const claimants = FALLBACK_ENGINES.filter((e) =>
      e.import.some((f) => f.extensions.includes("inp")),
    ).map((e) => e.key);
    expect(claimants).toEqual(["wds", "uds"]);
  });
});

// ── PROJECT_VIEWS ────────────────────────────────────────────────────────────

describe("PROJECT_VIEWS", () => {
  it("includes all expected WD views", () => {
    const ids = PROJECT_VIEWS.map((v) => v.id);
    expect(ids).toContain("overview");
    expect(ids).toContain("canvas");
    expect(ids).toContain("editor");
    expect(ids).toContain("analysis");
  });

  it("each view spec has a non-empty id and label", () => {
    for (const spec of PROJECT_VIEWS) {
      expect(spec.id.length).toBeGreaterThan(0);
      expect(spec.label.length).toBeGreaterThan(0);
    }
  });
});

// Keep compiler happy — ProjectView is used as a type constraint above
const _check: ProjectView = "canvas";
void _check;
