/**
 * The canvas ground control, where no basemap is possible.
 *
 * Built to the shape of {@link UnitSystemPicker}, because it answers the
 * same shape of question: a value that follows a setting made elsewhere
 * until this project pins it. The two menus therefore read alike — a
 * Default group whose one row names what the setting currently resolves to,
 * and an Override group whose rows stay put when that setting moves.
 *
 * The closed control shows the ground in effect, because that is the
 * question someone has while looking at the canvas. *Why* it is in effect
 * is the marker beside it and the grouping inside.
 */

import { ChevronUpDownIcon } from "@heroicons/react/16/solid";
import { useEffect, useRef, useState } from "react";
import {
  CANVAS_BACKGROUND_OVERRIDES,
  type CanvasBackground,
  effectiveCanvasBackground,
  GROUND_LABEL,
} from "../../../canvas/canvasBackground";
import {
  MenuGroupDivider,
  MenuGroupLabel,
  MenuRow,
} from "../../../components/ui/InheritanceMenu";
import { useResolvedTheme } from "../../../theme";

export function CanvasBackgroundPicker({
  value,
  onChange,
}: {
  value: CanvasBackground;
  onChange: (next: CanvasBackground) => void;
}) {
  const theme = useResolvedTheme();
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement | null>(null);
  const inherited = value === "theme";
  const effective = effectiveCanvasBackground(value, theme);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(e: PointerEvent) {
      if (wrapRef.current?.contains(e.target as Node)) return;
      setOpen(false);
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") setOpen(false);
    }
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const choose = (next: CanvasBackground) => {
    onChange(next);
    setOpen(false);
  };

  return (
    <div data-toolbar-dropdown ref={wrapRef} style={{ position: "relative" }}>
      <button
        type="button"
        className="tool-btn"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={(e) => {
          e.stopPropagation();
          setOpen((v) => !v);
        }}
        data-tooltip="Canvas background"
        data-tooltip-pos="bottom"
        style={{
          width: "auto",
          padding: "0 8px",
          fontSize: "var(--text-md)",
          gap: 4,
          display: "flex",
          alignItems: "center",
        }}
      >
        {GROUND_LABEL[effective]}
        {/* Why it is what it is, without opening the menu — and the answer
            to "I changed the theme, why did this project not follow?"

            "Default" rather than "Theme", though "Theme" would say more on
            its own: this marker names the group it points at, and the group
            is Default. The row's own description says what is being
            followed. The unit picker marks the same state with the same
            word, so a reader learns the pattern once rather than per
            control. */}
        {inherited && (
          <span style={{ color: "var(--text-tertiary)" }}>· Default</span>
        )}{" "}
        <ChevronUpDownIcon
          style={{ width: 12, height: 12, verticalAlign: "middle" }}
        />
      </button>

      {open && (
        <div
          role="menu"
          style={{
            position: "absolute",
            top: "calc(100% + 4px)",
            left: 0,
            zIndex: 20,
            width: 224,
            padding: "4px 0",
            borderRadius: 8,
            border: "1px solid var(--border)",
            background: "var(--bg-panel)",
            boxShadow: "var(--shadow-2)",
          }}
        >
          <MenuGroupLabel>Default</MenuGroupLabel>
          {/* Names what the theme currently resolves to, which "Match
              theme" alone cannot — and without it this row reads as a third
              explicit choice rather than as deference to the theme. The
              description is what separates it from the identical-looking
              override below whenever the two agree. */}
          <MenuRow
            label={GROUND_LABEL[theme]}
            description="Follows your app theme"
            selected={inherited}
            onSelect={() => choose("theme")}
          />
          <MenuGroupDivider />
          <MenuGroupLabel hint="fixed for this project">
            Override
          </MenuGroupLabel>
          {CANVAS_BACKGROUND_OVERRIDES.map((b) => (
            <MenuRow
              key={b}
              label={GROUND_LABEL[b]}
              selected={value === b}
              onSelect={() => choose(b)}
            />
          ))}
        </div>
      )}
    </div>
  );
}
