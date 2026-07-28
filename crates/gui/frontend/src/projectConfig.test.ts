import { describe, expect, it } from "vitest";
import { engineByKey, FALLBACK_ENGINES } from "./hooks/engines";
import type { ProjectView } from "./projectConfig";
import { ACCENT, PROJECT_VIEWS } from "./projectConfig";

// ── App accent + engine registry fallback ────────────────────────────────────

describe("app accent", () => {
  it("ACCENT is a 6-digit hex colour", () => {
    expect(ACCENT).toMatch(/^#[0-9a-fA-F]{6}$/);
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
    expect(engineByKey(FALLBACK_ENGINES, "och")).toBeNull();
    expect(engineByKey(FALLBACK_ENGINES, "")).toBeNull();
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
