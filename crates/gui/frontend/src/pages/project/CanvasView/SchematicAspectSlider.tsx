import {
  ArrowsRightLeftIcon,
  ArrowsUpDownIcon,
} from "@heroicons/react/16/solid";
import { aspectFactor } from "../../../canvas/schematicAspect";
import { CanvasSlider } from "./CanvasSlider";

const GLYPH = { width: 10, height: 10 } as const;

/**
 * Aspect control for the schematic layout: drag up to spread the layers
 * apart and tighten each layer, drag down for the reverse.
 *
 * One slider rather than one per axis. The two spacings' *uniform*
 * component is just a zoom — and the camera fit divides it out anyway — so
 * two independent tracks shared a single visible degree of freedom and
 * behaved as one control that reversed direction partway along. Trading the
 * axes against each other keeps their product at 1, which leaves exactly
 * the reshape zoom cannot do.
 */
export function SchematicAspectSlider({
  value,
  onChange,
}: {
  value: number;
  onChange: (next: number) => void;
}) {
  const factor = aspectFactor(value);
  const readout =
    factor > 1
      ? `${factor.toFixed(2)}× wider`
      : factor < 1
        ? `${(1 / factor).toFixed(2)}× taller`
        : "balanced";

  return (
    <CanvasSlider
      value={value}
      onChange={onChange}
      label="Layout aspect"
      readout={readout}
      hint="Drag up to spread layers, down to spread within layers"
      topGlyph={<ArrowsRightLeftIcon aria-hidden style={GLYPH} />}
      bottomGlyph={<ArrowsUpDownIcon aria-hidden style={GLYPH} />}
    />
  );
}
