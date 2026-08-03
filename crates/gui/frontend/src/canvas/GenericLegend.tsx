// ── Engine-generic legend ─────────────────────────────────────────────────────
// The catalog-driven implementation of the shared legend design (see
// `legend-primitives`): the same persistent glass control bar of variable
// pickers with a details popover of ramp bars that the wds legend has —
// only the content is engine-authored (`ResultMeta.generic`). No engine
// names, variable ids, units, or ranges are known here; the engine's
// catalog described them and this component draws exactly what it was
// given. Per-engine affordances the catalog does not describe (threshold
// editing, flow animation) are absent rather than faked.

import { ChevronUpDownIcon } from "@heroicons/react/16/solid";
import { useEffect, useRef, useState } from "react";
import {
  formatGenericValue,
  type GenericResultMeta,
  type GenericVariable,
  genericUnitLabel,
} from "../hooks/results";
import { useUnitSystem } from "../units";
import {
  LEGEND_BAR_STYLE,
  LEGEND_POPOVER_STYLE,
  LEGEND_ROOT_STYLE,
  LEGEND_SWATCH_BTN_STYLE,
  LinkGlyph,
  NodeGlyph,
  PickerButton,
  Ramp,
  RegionGlyph,
  SECTION_LABEL_STYLE,
} from "./legend-primitives";

export type GenericClassKey = "point" | "polyline" | "region";

/** Selected variable id per element class ("" = the catalog's first). */
export type GenericSelection = Record<GenericClassKey, string>;

/** CSS gradient for a ramp hint — mirrors `genericRgba`'s palettes. */
function rampGradient(ramp: GenericVariable["ramp"]): string {
  if (ramp === "diverging") {
    return "linear-gradient(90deg, rgb(37,99,235), rgb(203,213,225), rgb(201,64,64))";
  }
  if (ramp === "banded") {
    // Five hard steps, good → excessive (BAND_STEPS in colorUtils).
    const steps = [
      "rgb(61,175,117)",
      "rgb(163,190,84)",
      "rgb(212,160,23)",
      "rgb(201,120,64)",
      "rgb(201,64,64)",
    ];
    const stops = steps
      .map((c, i) => `${c} ${i * 20}% ${(i + 1) * 20}%`)
      .join(", ");
    return `linear-gradient(90deg, ${stops})`;
  }
  return "linear-gradient(90deg, rgb(166,200,240), rgb(21,74,158))";
}

/** Picker option label in the active display system: "Depth (ft)". */
function optionLabel(v: GenericVariable, sys: "si" | "us"): string {
  const unit = genericUnitLabel(v.quantity, sys);
  return unit ? `${v.label} (${unit})` : v.label;
}

interface ClassConfig {
  key: GenericClassKey;
  variables: GenericVariable[];
  glyph: React.ReactNode;
  pickerLabel: string;
}

/**
 * Legend for engines whose results are variable-keyed. One picker + ramp
 * section per element class that has catalog variables (the region section
 * appears only when the canvas shows region polygons).
 */
export function GenericLegend({
  meta,
  hasRegions,
  selection,
  onSelect,
}: {
  meta: GenericResultMeta;
  /** Whether the canvas is showing region polygons — hides the region
   * picker for models without areal elements. */
  hasRegions: boolean;
  selection: GenericSelection;
  onSelect: (cls: GenericClassKey, id: string) => void;
}) {
  const sys = useUnitSystem();
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [openPicker, setOpenPicker] = useState<GenericClassKey | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  // Same dismissal behaviour as the wds legend: any pointer press outside
  // the legend closes pickers and the details popover.
  useEffect(() => {
    function onPointerDown(e: PointerEvent) {
      if (rootRef.current?.contains(e.target as Node)) return;
      setOpenPicker(null);
      setDetailsOpen(false);
    }
    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, []);

  const classes: ClassConfig[] = [
    {
      key: "point" as const,
      variables: meta.pointVars,
      glyph: <NodeGlyph />,
      pickerLabel: "Node variable",
    },
    {
      key: "polyline" as const,
      variables: meta.polylineVars,
      glyph: <LinkGlyph />,
      pickerLabel: "Link variable",
    },
    ...(hasRegions
      ? [
          {
            key: "region" as const,
            variables: meta.regionVars,
            glyph: <RegionGlyph />,
            pickerLabel: "Region variable",
          },
        ]
      : []),
  ].filter((c) => c.variables.length > 0);

  const selected = (c: ClassConfig): GenericVariable =>
    c.variables.find((v) => v.id === selection[c.key]) ?? c.variables[0];

  return (
    <div ref={rootRef} style={LEGEND_ROOT_STYLE}>
      {/* ── Popover: one labelled gradient ramp per element class ── */}
      {detailsOpen && (
        <div
          className="legend-glass legend-glass--raised"
          style={LEGEND_POPOVER_STYLE}
        >
          {classes.map((c) => {
            const v = selected(c);
            return (
              <div key={c.key}>
                <div style={SECTION_LABEL_STYLE}>{optionLabel(v, sys)}</div>
                <Ramp
                  gradient={rampGradient(v.ramp)}
                  min={formatGenericValue(v.min, v.quantity, sys, false)}
                  max={formatGenericValue(v.max, v.quantity, sys, false)}
                />
              </div>
            );
          })}
        </div>
      )}

      {/* ── Persistent control bar: ramp swatches toggle + variable pickers ── */}
      <div
        className={`legend-glass${
          detailsOpen || openPicker != null ? " legend-glass--raised" : ""
        }`}
        style={LEGEND_BAR_STYLE}
      >
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            setDetailsOpen((v) => !v);
            setOpenPicker(null);
          }}
          title="Color scale"
          data-tooltip="Color scale"
          data-tooltip-pos="top"
          className="tool-btn"
          style={LEGEND_SWATCH_BTN_STYLE}
        >
          <div style={{ display: "flex", flexDirection: "column", gap: 3 }}>
            {classes.map((c) => (
              <div
                key={c.key}
                style={{
                  width: 28,
                  height: 5,
                  borderRadius: 3,
                  background: rampGradient(selected(c).ramp),
                }}
              />
            ))}
          </div>
          <ChevronUpDownIcon
            style={{ width: 10, height: 10, color: "var(--text-tertiary)" }}
          />
        </button>
        <div className="tool-divider" />
        {classes.map((c) => (
          <PickerButton
            key={c.key}
            value={selected(c).id}
            options={c.variables.map((v) => ({
              value: v.id,
              label: optionLabel(v, sys),
            }))}
            icon={c.glyph}
            pickerLabel={c.pickerLabel}
            isOpen={openPicker === c.key}
            onToggle={() => {
              setOpenPicker((p) => (p === c.key ? null : c.key));
              setDetailsOpen(false);
            }}
            onSelect={(id) => {
              onSelect(c.key, id);
              setOpenPicker(null);
            }}
          />
        ))}
      </div>
    </div>
  );
}
