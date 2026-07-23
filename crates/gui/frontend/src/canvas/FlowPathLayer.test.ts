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

  it("declares getFlowParams as an accessor defaulting to [1, 1, 0]", () => {
    const defaults = FlowPathLayer.defaultProps as Record<string, unknown>;
    expect(defaults.getFlowParams).toEqual({
      type: "accessor",
      value: [1, 1, 0],
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

  it("declares exactly one instanced flow attribute (packed vec3)", () => {
    const vsDecl = inject["vs:#decl"] ?? "";
    expect(vsDecl).toContain("in vec3 instanceFlowParams");
    // Exactly one instanced attribute declaration across all injections.
    const instanceDecls = allInjected.match(/in\s+vec\d\s+instance\w+/g) ?? [];
    expect(instanceDecls).toEqual(["in vec3 instanceFlowParams"]);
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
    expect(inject["fs:#decl"]).toContain("in vec3 vFlowParams");
  });

  it("includes the flowUniforms module carrying the clock as a UBO", () => {
    const flowModules = (shaders.modules ?? []).filter(
      (m) => m.name === "flow",
    );
    expect(flowModules).toHaveLength(1);
    const flow = flowModules[0];
    expect(flow.uniformTypes).toEqual({ time: "f32" });
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
