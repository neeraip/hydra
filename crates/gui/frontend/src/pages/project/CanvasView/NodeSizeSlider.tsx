import { nodeScaleFactor } from "../../../canvas/nodeScale";
import { CanvasSlider } from "./CanvasSlider";

/** Two dots, so the ends of the track say what they do without a word. */
function Dot({ size }: { size: number }) {
  return (
    <span
      aria-hidden
      style={{
        width: size,
        height: size,
        borderRadius: "50%",
        background: "currentColor",
        display: "block",
      }}
    />
  );
}

/**
 * Node size, relative to the size derived from the network's own spacing.
 *
 * The neutral midpoint is not an arbitrary default: it is the radius scaled
 * to how far apart this model's nodes actually are, so the slider is for
 * taste rather than for rescuing a model the fixed size suited badly. See
 * `nodeScale`.
 *
 * Shown in both views. A node's size relative to the links around it is the
 * one thing zoom cannot change, and that is as true of a plan as it is of a
 * schematic.
 */
export function NodeSizeSlider({
  value,
  onChange,
}: {
  value: number;
  onChange: (next: number) => void;
}) {
  const factor = nodeScaleFactor(value);
  const readout =
    factor > 1.005
      ? `${factor.toFixed(2)}× larger`
      : factor < 0.995
        ? `${(1 / factor).toFixed(2)}× smaller`
        : "scaled to the network";

  return (
    <CanvasSlider
      value={value}
      onChange={onChange}
      label="Node size"
      readout={readout}
      hint="Drag up for larger nodes, down for smaller"
      topGlyph={<Dot size={7} />}
      bottomGlyph={<Dot size={3} />}
    />
  );
}
