/**
 * BlendedSurfaceLayer — a SolidPolygonLayer that blends each cell's own
 * colour into its corners' on the GPU, one triangle per cell.
 *
 * The 2D surface is a field held per cell. Drawn plainly it is a mosaic:
 * one flat colour per triangle, claiming nothing between centres.
 * Blended, it should read as one surface while still showing each cell
 * the value the solver computed for it — so the picture is
 *
 *     colour = mix(corner colours, cell colour, 27·w₀·w₁·w₂)
 *
 * where `w` are barycentric coordinates. That weight is one at the
 * centroid, so a cell shows its own colour in its middle; zero on every
 * edge, so two cells sharing one interpolate between the same pair of
 * corner colours and agree along it; and smooth in between, evaluated
 * per pixel.
 *
 * Doing it here rather than on the CPU is what makes it affordable. The
 * previous version approximated the same curve by subdividing every cell
 * into a grid of 36 sub-triangles and colouring each corner: 270,000
 * polygons and 13 MB of geometry for a 7,500-cell mesh, rebuilt and
 * re-uploaded on every timeline step. This draws 7,500 triangles and
 * uploads two small colour arrays.
 *
 * The ramp stays in TypeScript. Both colours arrive already resolved by
 * `genericRgba`, so the legend and the map cannot drift: nothing here
 * knows what a ramp is.
 *
 * The plain fill uses this layer too, by handing it the same colour for
 * a cell and its corners — then the mix has nothing to do and the result
 * is flat. One layer, one geometry, and switching between the two costs
 * no rebuild.
 */

import type { DefaultProps } from "@deck.gl/core";
import {
  SolidPolygonLayer,
  type SolidPolygonLayerProps,
} from "@deck.gl/layers";
import { BLEND_BUBBLE_SCALE } from "./surfaceMesh";

export type BlendedSurfaceLayerProps<DataT = unknown> = {
  /** Per-vertex barycentric basis: (1,0,0), (0,1,0), (0,0,1) for a
   * cell's three vertices. Static for a mesh — supply it as a binary
   * attribute alongside the positions. */
  getBlendBary?: unknown;
  /** Per-vertex copy of the cell's own colour, the same for all three
   * vertices of a cell. Equal to the corner colours for a plain fill. */
  getBlendCellColor?: unknown;
} & SolidPolygonLayerProps<DataT>;

const defaultProps: DefaultProps<BlendedSurfaceLayerProps> = {
  getBlendBary: { type: "accessor", value: [1, 0, 0] },
  getBlendCellColor: { type: "accessor", value: [0, 0, 0, 0] },
};

export class BlendedSurfaceLayer<DataT = unknown> extends SolidPolygonLayer<
  DataT,
  BlendedSurfaceLayerProps<DataT>
> {
  static layerName = "BlendedSurfaceLayer";
  static override defaultProps = defaultProps;

  override initializeState(): void {
    super.initializeState();
    this.getAttributeManager()?.add({
      blendBary: {
        size: 3,
        type: "float32",
        accessor: "getBlendBary",
        defaultValue: [1, 0, 0],
      },
      blendCellColor: {
        size: 4,
        type: "unorm8",
        accessor: "getBlendCellColor",
        defaultValue: [0, 0, 0, 0],
      },
    });
  }

  override getShaders(type: "top" | "side") {
    // Forwarded, not assumed: an extruded polygon layer builds a second
    // model for its sides and asks for that model's shaders by name.
    // Answering "top" to both would draw the sides with the wrong vertex
    // shader. This layer never extrudes, so the case is latent — which
    // is exactly when a wrong answer survives unnoticed.
    const shaders = super.getShaders(type);
    return {
      ...shaders,
      inject: {
        ...shaders.inject,
        "vs:#decl": `
in vec3 blendBary;
in vec4 blendCellColor;
out vec3 vBlendBary;
out vec4 vBlendCellColor;
`,
        "vs:#main-end": `
  vBlendBary = blendBary;
  vBlendCellColor = blendCellColor;
`,
        "fs:#decl": `
in vec3 vBlendBary;
in vec4 vBlendCellColor;
`,
        // The bubble: one at the centroid, zero on every edge, smooth
        // between. Clamped because interpolation can leave a hair
        // outside the triangle along its edges.
        "fs:DECKGL_FILTER_COLOR": `
  float blendWeight = clamp(
    ${BLEND_BUBBLE_SCALE}.0 * vBlendBary.x * vBlendBary.y * vBlendBary.z,
    0.0, 1.0);
  color = mix(color, vBlendCellColor, blendWeight);
`,
      },
    };
  }
}
