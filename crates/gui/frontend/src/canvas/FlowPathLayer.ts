/**
 * FlowPathLayer — a deck.gl PathLayer subclass that injects GLSL shader code
 * to produce an animated wave/pulse effect along each pipe, giving a sense of
 * flow direction and magnitude without CPU-side particle management.
 *
 * Attribute budget: WebGL guarantees only 16 vertex attribute slots and the
 * base PathLayer already consumes 13, so the per-link inputs (speed, phase
 * offset) are packed into a single vec2 attribute and the global animation
 * clock is a uniform — separate attributes per input would fail shader
 * linking with "Too many attributes".
 *
 * Usage
 * -----
 *   new FlowPathLayer({
 *     data: linkData,
 *     getPath:       (d) => [d.from, d.to],            // static geometry
 *     flowTime:      animClock,                        // global clock (s)
 *     // speed sign encodes direction: negative animates to→from, so
 *     // reverse flow never requires re-tesselating the path geometry.
 *     getFlowParams: (d) => [signedSpeed(d), hashStr(d.id) * 6.283],
 *     updateTriggers: { getFlowParams: [flowMax, periodResult] },
 *   })
 */

import type { Accessor, DefaultProps, UpdateParameters } from "@deck.gl/core";
import { PathLayer, type PathLayerProps } from "@deck.gl/layers";

const flowUniformBlock = `\
layout(std140) uniform flowUniforms {
  float time;
  float pattern;
} flow;
`;

/** luma.gl shader module carrying the global animation clock as a UBO —
 * mirrors deck's own per-layer uniform pattern (see path-layer-uniforms). */
const flowUniforms = {
  name: "flow" as const,
  fs: flowUniformBlock,
  uniformTypes: { time: "f32" as const, pattern: "f32" as const },
};

export type FlowPathLayerProps<DataT = unknown> = {
  /** Global animation clock in seconds; drives the pulse phase. */
  flowTime?: number;
  /**
   * Which pattern the motion draws: `0` a continuous wave, `1` hard marks,
   * `2` soft parcels. Layer-wide rather than per-link — it says what kind of
   * claim the motion is making, which is a property of the variable on show
   * and not of any one pipe. A uniform also keeps it off the attribute
   * budget.
   */
  flowPattern?: number;
  /** Per-link [speed -1..1 (sign = direction along the path), phaseOffset
   * radians]. */
  getFlowParams?: Accessor<DataT, [number, number]>;
} & PathLayerProps<DataT>;

const defaultProps: DefaultProps<FlowPathLayerProps> = {
  flowTime: { type: "number", value: 0 },
  flowPattern: { type: "number", value: 0 },
  getFlowParams: {
    type: "accessor",
    value: [1, 0] as [number, number],
  },
};

export class FlowPathLayer<DataT = unknown> extends PathLayer<
  DataT,
  FlowPathLayerProps<DataT>
> {
  static layerName = "FlowPathLayer";
  static override defaultProps = defaultProps;

  override initializeState(): void {
    super.initializeState();
    this.getAttributeManager()?.addInstanced({
      instanceFlowParams: {
        size: 2,
        accessor: "getFlowParams",
        defaultValue: [1, 0],
      },
    });
  }

  override updateState(params: UpdateParameters<this>): void {
    super.updateState(params);
    const updateTriggers = params.changeFlags.updateTriggersChanged;
    if (
      updateTriggers &&
      (updateTriggers.all || updateTriggers.getFlowParams)
    ) {
      this.getAttributeManager()?.invalidate("instanceFlowParams");
    }
  }

  override getShaders() {
    const shaders = super.getShaders();
    return {
      ...shaders,
      modules: [...(shaders.modules ?? []), flowUniforms],
      inject: {
        ...shaders.inject,
        "vs:#decl": `
      in vec2 instanceFlowParams;
      out vec2 vFlowParams;
      `,
        "vs:#main-end": `
      vFlowParams = instanceFlowParams;
      `,
        "fs:#decl": `
      in vec2 vFlowParams;
`,
        "fs:DECKGL_FILTER_COLOR": `
      float flowSpeed = abs(vFlowParams.x);
      float flowDir = vFlowParams.x < 0.0 ? -1.0 : 1.0;
      float flowPhaseOffset = vFlowParams.y;
      float pathCoord = geometry.uv.y;
      float crossPos = abs(geometry.uv.x);

      // Use raw path coordinate (not normalised) so animation remains valid
      // regardless of how the path module parameterises uv.y on this platform.
      // 0.083 = the old 0.055 + 0.028 * flowFrequency, where flowFrequency
      // was always 1.0 — a constant that cost a float per link in an
      // instanced buffer and conveyed nothing.
      // The clock wraps at 3600s (see flowAnimRef). For that wrap to be
      // invisible, every link's time coefficient must turn a whole number of
      // cycles in 3600s — otherwise each link's dashes jump once an hour, by
      // a different amount each. Quantising the coefficient to 0.05 steps
      // guarantees it: 3600 * 0.05 = 180, so 3600 * k is always an integer.
      // 19 steps across a 0.95–1.85 range is far finer than the eye resolves.
      // A speed of exactly zero means still, and still has to look still.
      // Left to the expression below it would resolve to 0.95 — nearly the
      // rate of a full-speed link, since the whole range is only 0.95–1.85 —
      // so a link carrying nothing would march along at almost full pelt.
      // That was survivable while every animated variable was a rate, and is
      // not now: telling a stationary link from a moving one is the entire
      // content of the discrete pattern.
      float flowRate = flowSpeed <= 0.0
        ? 0.0
        : 0.95 + 0.05 * floor(flowSpeed * 18.0 + 0.5);
      float phase = pathCoord * 0.083
        - flow.time * flowRate * flowDir
        + flowPhaseOffset;

      float pulse;
      if (flow.pattern > 1.5) {
        // Soft parcels, for motion that means "carrying" rather than
        // "carrying at this rate". Discrete, so it cannot be read as a
        // continuous quantity; soft-edged, so it cannot be confused with the
        // hard marks below, which answer a different question. Measured from
        // the middle of each period outward, giving a bump about 0.44 of the
        // period wide with clear water between one and the next.
        float c = abs(fract(phase) - 0.5);
        pulse = 1.0 - smoothstep(0.0, 0.22, c);
      } else if (flow.pattern > 0.5) {
        // Discrete marks, for motion that means "carrying" rather than
        // "carrying this much". Hard edges read as countable; the wave below
        // reads as a scale, which is a claim a categorical legend cannot
        // support. 0.34 of each period is marked, with an edge just soft
        // enough that the leading face does not crawl with aliasing.
        float f = fract(phase);
        pulse = smoothstep(0.0, 0.06, f) - smoothstep(0.28, 0.34, f);
      } else {
        // Two sines beating against each other: a travelling swell with no
        // hard edge anywhere, which is what a continuous quantity should
        // look like.
        float w1 = 0.5 + 0.5 * sin(6.28318530718 * phase);
        float w2 = 0.5 + 0.5 * sin(6.28318530718 * (phase * 1.61803398875 + 0.21));
        pulse = clamp(0.72 * w1 + 0.28 * w2, 0.0, 1.0);
      }

      // A still link draws solid at full colour rather than at the pattern's
      // trough: it is not mid-cycle, it has no cycle, and dimming it would
      // read as a third state that means nothing.
      if (flowRate <= 0.0) {
        pulse = 1.0;
      }

      float widthCore = 1.0 - smoothstep(0.45, 1.0, crossPos);
      float intensity = max(0.22, (0.22 + 0.78 * pulse) * (0.40 + 0.60 * widthCore));

      color.rgb *= (0.92 + 0.18 * pulse);
      color.a *= intensity;
`,
      },
    };
  }

  override draw(opts: Parameters<PathLayer<DataT>["draw"]>[0]): void {
    const model = (
      this.state as {
        model?: {
          shaderInputs: {
            setProps(props: Record<string, Record<string, number>>): void;
          };
        };
      }
    ).model;
    model?.shaderInputs.setProps({
      flow: {
        time: this.props.flowTime ?? 0,
        pattern: this.props.flowPattern ?? 0,
      },
    });
    super.draw(opts);
  }
}
