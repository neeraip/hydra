/**
 * FlowPathLayer — a deck.gl PathLayer subclass that injects GLSL shader code
 * to produce an animated wave/pulse effect along each pipe, giving a sense of
 * flow direction and magnitude without CPU-side particle management.
 *
 * Usage
 * -----
 *   new FlowPathLayer({
 *     data: linkData,
 *     getPath:           (d) => d.flow < 0 ? [d.to, d.from] : [d.from, d.to],
 *     getFlowTime:       () => animClock,             // global tick → same for all
 *     getFlowSpeed:      (d) => normalizedVelocity(d),
 *     getFlowFrequency:  (_d) => 1.0,
 *     getFlowPhaseOffset:(d) => hashStr(d.id) * 6.283, // random per-link offset
 *     updateTriggers: { getFlowTime: [animClock], getFlowSpeed: [flowMax] },
 *   })
 */

import type { UpdateParameters } from "@deck.gl/core";
import { PathLayer } from "@deck.gl/layers";

export interface FlowPathLayerProps {
  getFlowTime: () => number;
  getFlowSpeed: (d: any) => number;
  getFlowFrequency: (d: any) => number;
  getFlowPhaseOffset: (d: any) => number;
}

export class FlowPathLayer<DataT = unknown> extends PathLayer<
  DataT,
  FlowPathLayerProps
> {
  static layerName = "FlowPathLayer";

  override initializeState(): void {
    super.initializeState();
    this.getAttributeManager()?.addInstanced({
      instanceFlowTimes: { size: 1, accessor: "getFlowTime", defaultValue: 0 },
      instanceFlowSpeeds: {
        size: 1,
        accessor: "getFlowSpeed",
        defaultValue: 1,
      },
      instanceFlowFrequencies: {
        size: 1,
        accessor: "getFlowFrequency",
        defaultValue: 1,
      },
      instanceFlowPhaseOffsets: {
        size: 1,
        accessor: "getFlowPhaseOffset",
        defaultValue: 0,
      },
    });
  }

  override updateState(params: UpdateParameters<this>): void {
    super.updateState(params);
    const updateTriggers = params.changeFlags.updateTriggersChanged;
    if (
      updateTriggers &&
      (updateTriggers.all ||
        updateTriggers.getFlowTime ||
        updateTriggers.getFlowSpeed ||
        updateTriggers.getFlowFrequency ||
        updateTriggers.getFlowPhaseOffset)
    ) {
      this.getAttributeManager()?.invalidate("instanceFlowTimes");
      this.getAttributeManager()?.invalidate("instanceFlowSpeeds");
      this.getAttributeManager()?.invalidate("instanceFlowFrequencies");
      this.getAttributeManager()?.invalidate("instanceFlowPhaseOffsets");
    }
  }

  override getShaders() {
    const shaders = super.getShaders();
    return {
      ...shaders,
      inject: {
        ...shaders.inject,
        "vs:#decl": `
      #if __VERSION__ >= 300
      #define FLOW_ATTRIBUTE in
      #define FLOW_VARYING out
      #else
      #define FLOW_ATTRIBUTE attribute
      #define FLOW_VARYING varying
      #endif

      FLOW_ATTRIBUTE float instanceFlowTimes;
      FLOW_ATTRIBUTE float instanceFlowSpeeds;
      FLOW_ATTRIBUTE float instanceFlowFrequencies;
      FLOW_ATTRIBUTE float instanceFlowPhaseOffsets;
      FLOW_VARYING float vFlowTime;
      FLOW_VARYING float vFlowSpeed;
      FLOW_VARYING float vFlowFrequency;
      FLOW_VARYING float vFlowPhaseOffset;
      `,
        "vs:#main-end": `
      vFlowTime = instanceFlowTimes;
      vFlowSpeed = instanceFlowSpeeds;
      vFlowFrequency = instanceFlowFrequencies;
      vFlowPhaseOffset = instanceFlowPhaseOffsets;
      `,
        "fs:#decl": `
      #if __VERSION__ >= 300
      #define FLOW_VARYING_IN in
      #else
      #define FLOW_VARYING_IN varying
      #endif

      FLOW_VARYING_IN float vFlowTime;
      FLOW_VARYING_IN float vFlowSpeed;
      FLOW_VARYING_IN float vFlowFrequency;
      FLOW_VARYING_IN float vFlowPhaseOffset;
`,
        "fs:DECKGL_FILTER_COLOR": `
      float pathCoord = geometry.uv.y;
      float cross = abs(geometry.uv.x);

      // Use raw path coordinate (not normalised) so animation remains valid
      // regardless of how the path module parameterises uv.y on this platform.
      float phase = pathCoord * (0.055 + 0.028 * vFlowFrequency)
        - vFlowTime * (0.95 + 0.90 * vFlowSpeed)
        + vFlowPhaseOffset;

      float w1 = 0.5 + 0.5 * sin(6.28318530718 * phase);
      float w2 = 0.5 + 0.5 * sin(6.28318530718 * (phase * 1.61803398875 + 0.21));
      float pulse = clamp(0.72 * w1 + 0.28 * w2, 0.0, 1.0);

      float widthCore = 1.0 - smoothstep(0.45, 1.0, cross);
      float intensity = max(0.22, (0.22 + 0.78 * pulse) * (0.40 + 0.60 * widthCore));

      color.rgb *= (0.92 + 0.18 * pulse);
      color.a *= intensity;
`,
      },
    };
  }
}
