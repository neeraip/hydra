/**
 * Static contract tests for FlowPathLayer's attribute-budget fix.
 *
 * WebGL guarantees only 16 vertex attribute slots and the base PathLayer
 * already consumes 13, so the per-link flow inputs must stay packed into a
 * single vec3 instanced attribute (`instanceFlowParams`) with the animation
 * clock as a uniform. These tests lock in that shader/props contract so a
 * refactor back to one-attribute-per-input (which fails shader linking with
 * "Too many attributes") cannot land silently.
 *
 * `getShaders()` never touches the GL device — it only reads
 * `this.context.defaultShaderModules` — so it is called here on a bare
 * instance with a stubbed layer context.
 */
import { describe, expect, it } from "vitest";
import { FlowPathLayer } from "./FlowPathLayer";

const LEGACY_ATTRIBUTES = [
  "instanceFlowTimes",
  "instanceFlowSpeeds",
  "instanceFlowFrequencies",
  "instancePhaseOffsets",
];

interface ShaderModule {
  name: string;
  fs?: string;
  uniformTypes?: Record<string, string>;
}

interface Shaders {
  modules?: ShaderModule[];
  inject?: Record<string, string>;
}

function getShadersOnBareInstance(): Shaders {
  const layer = new FlowPathLayer({});
  // Layer.getShaders reads only context.defaultShaderModules; no GL device.
  (
    layer as unknown as { context: { defaultShaderModules: unknown[] } }
  ).context = { defaultShaderModules: [] };
  return layer.getShaders() as Shaders;
}

describe("FlowPathLayer statics", () => {
  it("declares flowTime as a number prop defaulting to 0", () => {
    const defaults = FlowPathLayer.defaultProps as Record<string, unknown>;
    expect(defaults.flowTime).toEqual({ type: "number", value: 0 });
  });

  it("declares getFlowParams as an accessor defaulting to [1, 0]", () => {
    const defaults = FlowPathLayer.defaultProps as Record<string, unknown>;
    expect(defaults.getFlowParams).toEqual({
      type: "accessor",
      value: [1, 0],
    });
  });

  it("has a stable layerName", () => {
    expect(FlowPathLayer.layerName).toBe("FlowPathLayer");
  });
});

describe("FlowPathLayer.getShaders", () => {
  const shaders = getShadersOnBareInstance();
  const inject = shaders.inject ?? {};
  const allInjected = Object.values(inject).join("\n");

  it("declares exactly one instanced flow attribute (packed vec2)", () => {
    const vsDecl = inject["vs:#decl"] ?? "";
    expect(vsDecl).toContain("in vec2 instanceFlowParams");
    // Exactly one instanced attribute declaration across all injections.
    const instanceDecls = allInjected.match(/in\s+vec\d\s+instance\w+/g) ?? [];
    expect(instanceDecls).toEqual(["in vec2 instanceFlowParams"]);
  });

  it("does not reintroduce the four legacy per-input attributes", () => {
    const moduleSources = (shaders.modules ?? [])
      .map((m) => m.fs ?? "")
      .join("\n");
    for (const legacy of LEGACY_ATTRIBUTES) {
      expect(allInjected).not.toContain(legacy);
      expect(moduleSources).not.toContain(legacy);
    }
  });

  it("forwards the packed params to the fragment stage as a varying", () => {
    expect(inject["vs:#main-end"]).toContain(
      "vFlowParams = instanceFlowParams",
    );
    expect(inject["fs:#decl"]).toContain("in vec2 vFlowParams");
  });

  it("includes the flowUniforms module carrying the clock as a UBO", () => {
    const flowModules = (shaders.modules ?? []).filter(
      (m) => m.name === "flow",
    );
    expect(flowModules).toHaveLength(1);
    const flow = flowModules[0];
    // Exact, so a per-link input cannot be smuggled in here as a uniform
    // either: the block is the clock and the pattern, both layer-wide.
    expect(flow.uniformTypes).toEqual({ time: "f32", pattern: "f32" });
    expect(flow.fs).toContain("uniform flowUniforms");
    expect(flow.fs).toContain("float time");
  });

  it("animates from the uniform clock with a signed direction term", () => {
    const fsColor = inject["fs:DECKGL_FILTER_COLOR"] ?? "";
    // Clock comes from the uniform block, never an attribute.
    expect(fsColor).toContain("flow.time");
    // Sign of vFlowParams.x encodes flow direction (negative = to→from).
    expect(fsColor).toContain("vFlowParams.x < 0.0 ? -1.0 : 1.0");
    expect(fsColor).toContain("abs(vFlowParams.x)");
  });
});

// ── Clock wrap ───────────────────────────────────────────────────────────────

describe("flow rate quantisation", () => {
  /** The shader's `flowRate`, mirrored so the wrap invariant is testable. */
  const flowRate = (speed: number) =>
    speed <= 0 ? 0 : 0.95 + 0.05 * Math.floor(speed * 18 + 0.5);

  const WRAP_SECONDS = 3600;

  it("turns a whole number of cycles before the clock wraps", () => {
    // The animation clock resets at 3600s. Unless every link's rate completes
    // an exact number of cycles by then, each one's dashes jump at the wrap —
    // by a different amount each, once an hour.
    for (let i = 0; i <= 40; i += 1) {
      const cycles = flowRate(i / 40) * WRAP_SECONDS;
      expect(Number.isInteger(Math.round(cycles * 1e6) / 1e6)).toBe(true);
    }
  });

  it("keeps every moving rate inside the intended range", () => {
    expect(flowRate(0.001)).toBeCloseTo(0.95, 10);
    expect(flowRate(1)).toBeCloseTo(1.85, 10);
  });

  /**
   * Zero is not the bottom of the range, it is outside it.
   *
   * The range spans 0.95–1.85, so a link left to fall through to its lower
   * end would march at nearly the rate of one at full speed. That was
   * tolerable while everything animated was a rate. It is not now: whether a
   * link is moving at all is the whole content of the discrete pattern.
   */
  it("stops a link that is not moving", () => {
    expect(flowRate(0)).toBe(0);
    expect(flowRate(-0)).toBe(0);
  });

  /**
   * And the shader agrees.
   *
   * `flowRate` above is a mirror, written in JavaScript so the wrap
   * invariant can be reasoned about at all. A mirror cannot see the thing it
   * mirrors: the guard was once removed from the GLSL and every assertion
   * against the copy still passed. So the source is checked directly for the
   * two decisions the mirror cannot vouch for.
   */
  it("guards against zero in the shader itself, not only in the mirror", () => {
    const fs =
      getShadersOnBareInstance().inject?.["fs:DECKGL_FILTER_COLOR"] ?? "";
    expect(fs).toContain("flowSpeed <= 0.0");
    expect(fs).toContain("0.95 + 0.05 * floor(flowSpeed * 18.0 + 0.5)");
  });

  it("resolves finely enough not to band visibly", () => {
    // 19 steps across the range; adjacent speeds differ by at most one step.
    // Sampled above zero, which is now stillness rather than the slowest rate.
    const steps = new Set(
      Array.from({ length: 100 }, (_, i) => flowRate((i + 1) / 100)),
    );
    expect(steps.size).toBe(19);
  });
});

/**
 * The pattern the motion draws.
 *
 * Magnitude and presence used to render identically — the same swell at
 * different rates — so the constant rate deliberately kept out of the data
 * was reintroduced by the pixels, and a categorical legend appeared to
 * claim every open link moves at the same speed.
 *
 * Layer-wide rather than per-link on purpose: it says what kind of claim the
 * motion makes, which belongs to the variable on show and not to any one
 * pipe. Per-link it would also cost an attribute, and the whole reason
 * `instanceFlowParams` is packed is that there are none to spare.
 */
describe("FlowPathLayer pattern selection", () => {
  it("declares flowPattern as a number prop defaulting to the wave", () => {
    const prop = (
      FlowPathLayer.defaultProps as unknown as {
        flowPattern: { type: string; value: number };
      }
    ).flowPattern;
    expect(prop.type).toBe("number");
    expect(prop.value).toBe(0);
  });

  it("carries the pattern as a uniform, not an attribute", () => {
    const shaders = getShadersOnBareInstance();
    const mod = shaders.modules?.find((m) => m.name === "flow");
    expect(mod?.uniformTypes?.pattern).toBe("f32");
    expect(mod?.fs).toContain("float pattern;");
    // An attribute here would be one more of the three slots left.
    expect(shaders.inject?.["vs:#decl"] ?? "").not.toContain("Pattern");
  });

  it("draws one of three patterns, chosen by that uniform", () => {
    const fs =
      getShadersOnBareInstance().inject?.["fs:DECKGL_FILTER_COLOR"] ?? "";
    // Three claims about the motion, so three branches on the one uniform.
    expect(fs.match(/flow\.pattern/g)?.length).toBe(2);
    expect(fs).toContain("flow.pattern > 1.5");
    expect(fs).toContain("flow.pattern > 0.5");
    // Soft parcels: a bump measured out from the middle of each period.
    expect(fs).toContain("abs(fract(phase) - 0.5)");
    // Hard marks: a step across it. Continuous swell: the beating sines.
    expect(fs).toContain("fract(phase)");
    expect(fs).toContain("sin(6.28318530718 * phase)");
  });

  /**
   * The parcels have to be soft and the marks hard, or the two patterns
   * answer different questions while looking like each other. Softness is
   * the interpolated edge; hardness is the edge being a step of a few
   * hundredths.
   */
  it("edges the parcels softly and the marks sharply", () => {
    const fs =
      getShadersOnBareInstance().inject?.["fs:DECKGL_FILTER_COLOR"] ?? "";
    expect(fs).toContain("smoothstep(0.0, 0.22, c)");
    expect(fs).toContain("smoothstep(0.0, 0.06, f)");
  });

  /** Both patterns read the same phase, so the clock-wrap invariant above
   *  covers the marks as well as the wave. */
  it("advances both patterns from the same phase", () => {
    const fs =
      getShadersOnBareInstance().inject?.["fs:DECKGL_FILTER_COLOR"] ?? "";
    expect(fs.match(/float phase =/g)?.length).toBe(1);
  });

  /** A still link is not mid-cycle; it has no cycle. */
  it("draws a still link solid rather than at the pattern's trough", () => {
    const fs =
      getShadersOnBareInstance().inject?.["fs:DECKGL_FILTER_COLOR"] ?? "";
    expect(fs).toContain("if (flowRate <= 0.0)");
    expect(fs).toContain("pulse = 1.0;");
  });
});
