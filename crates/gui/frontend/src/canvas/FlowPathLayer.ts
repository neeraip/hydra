/**
 * FlowPathLayer — a deck.gl PathLayer subclass that injects GLSL shader code
 * to produce an animated wave/pulse effect along each pipe, giving a sense of
 * flow direction and magnitude without CPU-side particle management.
 *
 * Attribute budget: WebGL guarantees only 16 vertex attribute slots and the
 * base PathLayer already consumes 13, so the per-link inputs (speed,
 * frequency, phase offset) are packed into a single vec3 attribute and the
 * global animation clock is a uniform — separate attributes per input would
 * fail shader linking with "Too many attributes".
 *
 * Usage
 * -----
 *   new FlowPathLayer({
 *     data: linkData,
 *     getPath:       (d) => [d.from, d.to],            // static geometry
 *     flowTime:      animClock,                        // global clock (s)
 *     // speed sign encodes direction: negative animates to→from, so
 *     // reverse flow never requires re-tesselating the path geometry.
 *     getFlowParams: (d) => [signedSpeed(d), 1, hashStr(d.id) * 6.283],
 *     updateTriggers: { getFlowParams: [flowMax, periodResult] },
 *   })
 */

import type { Accessor, DefaultProps, UpdateParameters } from "@deck.gl/core";
import { PathLayer, type PathLayerProps } from "@deck.gl/layers";

const flowUniformBlock = `\
layout(std140) uniform flowUniforms {
  float time;
} flow;
`;

/** luma.gl shader module carrying the global animation clock as a UBO —
 * mirrors deck's own per-layer uniform pattern (see path-layer-uniforms). */
const flowUniforms = {
  name: "flow" as const,
  fs: flowUniformBlock,
  uniformTypes: { time: "f32" as const },
};

export type FlowPathLayerProps<DataT = unknown> = {
  /** Global animation clock in seconds; drives the pulse phase. */
  flowTime?: number;
  /** Per-link [speed -1..1 (sign = direction along the path), frequency,
   * phaseOffset radians]. */
  getFlowParams?: Accessor<DataT, [number, number, number]>;
} & PathLayerProps<DataT>;

const defaultProps: DefaultProps<FlowPathLayerProps> = {
  flowTime: { type: "number", value: 0 },
  getFlowParams: {
    type: "accessor",
    value: [1, 1, 0] as [number, number, number],
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
        size: 3,
        accessor: "getFlowParams",
        defaultValue: [1, 1, 0],
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
      in vec3 instanceFlowParams;
      out vec3 vFlowParams;
      `,
        "vs:#main-end": `
      vFlowParams = instanceFlowParams;
      `,
        "fs:#decl": `
      in vec3 vFlowParams;
`,
        "fs:DECKGL_FILTER_COLOR": `
      float flowSpeed = abs(vFlowParams.x);
      float flowDir = vFlowParams.x < 0.0 ? -1.0 : 1.0;
      float flowFrequency = vFlowParams.y;
      float flowPhaseOffset = vFlowParams.z;
      float pathCoord = geometry.uv.y;
      float crossPos = abs(geometry.uv.x);

      // Use raw path coordinate (not normalised) so animation remains valid
      // regardless of how the path module parameterises uv.y on this platform.
      float phase = pathCoord * (0.055 + 0.028 * flowFrequency)
        - flow.time * (0.95 + 0.90 * flowSpeed) * flowDir
        + flowPhaseOffset;

      float w1 = 0.5 + 0.5 * sin(6.28318530718 * phase);
      float w2 = 0.5 + 0.5 * sin(6.28318530718 * (phase * 1.61803398875 + 0.21));
      float pulse = clamp(0.72 * w1 + 0.28 * w2, 0.0, 1.0);

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
    model?.shaderInputs.setProps({ flow: { time: this.props.flowTime ?? 0 } });
    super.draw(opts);
  }
}
