import { describe, expect, it } from "vitest";
import type { ProjectView } from "./projectConfig";
import { ACCENT, LABEL, PILL, PROJECT_VIEWS } from "./projectConfig";

// ── WD engine constants ────────────────────────────────────────────────────

describe("WD engine constants", () => {
  it("LABEL is non-empty", () => {
    expect(LABEL.length).toBeGreaterThan(0);
  });

  it("PILL is 'WD'", () => {
    expect(PILL).toBe("WD");
  });

  it("ACCENT is a 6-digit hex colour", () => {
    expect(ACCENT).toMatch(/^#[0-9a-fA-F]{6}$/);
  });
});

// ── PROJECT_VIEWS ────────────────────────────────────────────────────────────

describe("PROJECT_VIEWS", () => {
  it("includes all expected WD views", () => {
    const ids = PROJECT_VIEWS.map((v) => v.id);
    expect(ids).toContain("overview");
    expect(ids).toContain("canvas");
    expect(ids).not.toContain("analysis");
    expect(ids).not.toContain("editor");
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
