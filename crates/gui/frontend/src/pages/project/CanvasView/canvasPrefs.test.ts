import { describe, expect, it } from "vitest";
import {
  CANVAS_PREF_DEFAULTS,
  type CanvasPrefs,
  resolveCanvasPrefs,
} from "./canvasPrefs";

/**
 * Restoring a project's canvas.
 *
 * A dozen fallbacks taken together, which lived inside an effect and so
 * could not be called by anything. The one defect it has already produced
 * is the first test here: preferences were applied only where a stored
 * value was present and valid, so a project that had never saved any kept
 * whatever the *previous* project was showing — and the persist effect then
 * wrote that under the new project's key, which made the bleed permanent
 * rather than merely visible.
 */

const KEYS = Object.keys(CANVAS_PREF_DEFAULTS) as (keyof CanvasPrefs)[];

describe("a project with nothing saved", () => {
  /**
   * The bleed. Every preference has to be answered, not just the stored
   * ones — an unanswered preference is the previous project's, and the
   * canvas keeps showing it.
   */
  it("answers for every preference, not only the stored ones", () => {
    const resolved = resolveCanvasPrefs(null);
    for (const key of KEYS) {
      expect(resolved[key], key).toBeDefined();
    }
    expect(Object.keys(resolved).sort()).toEqual([...KEYS].sort());
  });

  it("comes back as the defaults", () => {
    expect(resolveCanvasPrefs(null)).toEqual({
      ...CANVAS_PREF_DEFAULTS,
      genericSelection: {
        point: CANVAS_PREF_DEFAULTS.nodeVar,
        polyline: CANVAS_PREF_DEFAULTS.linkVar,
        region: "",
      },
    });
  });

  /** A half-written pref object is the same case, one key at a time. */
  it("fills the gaps around whatever was stored", () => {
    const resolved = resolveCanvasPrefs({ viewMode: "schematic" });
    expect(resolved.viewMode).toBe("schematic");
    expect(resolved.basemap).toBe(CANVAS_PREF_DEFAULTS.basemap);
    expect(resolved.scaleMode).toBe(CANVAS_PREF_DEFAULTS.scaleMode);
  });
});

describe("a stored value that is not one of the allowed ones", () => {
  /** localStorage is editable by hand and survives across versions. */
  it("falls back rather than passing it through", () => {
    const junk = {
      viewMode: "sideways",
      legendOpen: "yes",
      nodeVar: "nonsense",
      linkVar: 7,
    } as unknown as Partial<CanvasPrefs>;
    const resolved = resolveCanvasPrefs(junk);
    expect(resolved.viewMode).toBe(CANVAS_PREF_DEFAULTS.viewMode);
    expect(resolved.legendOpen).toBe(CANVAS_PREF_DEFAULTS.legendOpen);
    expect(resolved.nodeVar).toBe(CANVAS_PREF_DEFAULTS.nodeVar);
    expect(resolved.linkVar).toBe(CANVAS_PREF_DEFAULTS.linkVar);
  });

  it("clamps the sliders instead of refusing them", () => {
    const wild = {
      basemapOpacity: 40,
      schematicAspect: -999,
    } as Partial<CanvasPrefs>;
    const resolved = resolveCanvasPrefs(wild);
    expect(resolved.basemapOpacity).toBeLessThanOrEqual(1);
    expect(resolved.basemapOpacity).toBeGreaterThanOrEqual(0);
    expect(Number.isFinite(resolved.schematicAspect)).toBe(true);
  });

  it("keeps a basemap id the allowlists cannot know about", () => {
    // Provider styles are open-ended, so this one is checked structurally.
    const resolved = resolveCanvasPrefs({
      basemap: "provider:esri:world-imagery",
    });
    expect(resolved.basemap).toBe("provider:esri:world-imagery");
  });
});

describe("a variable selection", () => {
  /**
   * Not validated against a catalog, on purpose: which variables exist
   * depends on the run that produced the results, so an unfamiliar id is a
   * choice made against a different one rather than corruption. The legend
   * falls back to its first variable for those.
   */
  it("survives an id no current catalog holds", () => {
    const resolved = resolveCanvasPrefs({
      genericSelection: { point: "depth", polyline: "capacity", region: "" },
    });
    expect(resolved.genericSelection.point).toBe("depth");
    expect(resolved.genericSelection.polyline).toBe("capacity");
  });

  /** Prefs written before the legend became the single store have only
   *  the old pair, and must not come back empty. */
  it("is seeded from the legacy pair when none was saved", () => {
    const resolved = resolveCanvasPrefs({ nodeVar: "head", linkVar: "flow" });
    expect(resolved.genericSelection.point).toBe("head");
    expect(resolved.genericSelection.polyline).toBe("flow");
  });

  /** The saved selection wins where there is one — it is the newer store. */
  it("prefers what was saved over the legacy pair", () => {
    const resolved = resolveCanvasPrefs({
      nodeVar: "head",
      linkVar: "flow",
      genericSelection: { point: "quality", polyline: "status", region: "" },
    });
    expect(resolved.genericSelection.point).toBe("quality");
    expect(resolved.genericSelection.polyline).toBe("status");
  });

  /** A legacy pair that is itself junk seeds the default, not the junk. */
  it("does not seed an invalid legacy value", () => {
    const resolved = resolveCanvasPrefs({
      nodeVar: "nonsense",
    } as unknown as Partial<CanvasPrefs>);
    expect(resolved.genericSelection.point).toBe(CANVAS_PREF_DEFAULTS.nodeVar);
  });
});

describe("the scale mode and the criteria toggle", () => {
  /** Prefs written before the two keys merged, read as two again. */
  it("migrates the pre-merge pair, keeping both answers", () => {
    const judging = resolveCanvasPrefs({
      colorMode: "threshold",
    } as unknown as CanvasPrefs);
    expect(judging.criteriaScale).toBe(true);
    expect(judging.scaleMode).toBe("run");

    const stepped = resolveCanvasPrefs({
      rangeMode: "step",
    } as unknown as CanvasPrefs);
    expect(stepped.scaleMode).toBe("step");
    expect(stepped.criteriaScale).toBe(false);

    // The combination the merge could not hold: both were saved, and the
    // one slot it had went to criteria.
    const both = resolveCanvasPrefs({
      colorMode: "threshold",
      rangeMode: "step",
    } as unknown as CanvasPrefs);
    expect(both.scaleMode).toBe("step");
    expect(both.criteriaScale).toBe(true);
  });

  it("carries a merged criteria mode across the split", () => {
    const resolved = resolveCanvasPrefs({
      scaleMode: "criteria",
    } as unknown as CanvasPrefs);
    expect(resolved.criteriaScale).toBe(true);
    expect(resolved.scaleMode).toBe("run");
  });

  it("prefers the current keys over the shapes they replaced", () => {
    const resolved = resolveCanvasPrefs({
      scaleMode: "run",
      criteriaScale: false,
      colorMode: "threshold",
    } as unknown as CanvasPrefs);
    expect(resolved.scaleMode).toBe("run");
    expect(resolved.criteriaScale).toBe(false);
  });
});
