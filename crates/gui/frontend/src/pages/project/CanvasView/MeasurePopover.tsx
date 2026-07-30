import { XMarkIcon } from "@heroicons/react/16/solid";
import type { MeasurePoint } from "../../../canvas/measureSnap";
import { MiddleTruncate } from "../../../components/ui/MiddleTruncate";
import { TypeBadge } from "../../../components/ui/TypeBadge";
import { formatDistance, formatLatLng, useUnitSystem } from "../../../units";

/**
 * One end of the measurement: the element's letter badge and id, or a plain
 * label when the point is not attached to anything.
 *
 * The badge replaces the words "node"/"link" — it says more in less space, since
 * it distinguishes a pipe from a pump where "link" could not, and it is the same
 * glyph the element list and inspector already use for that element.
 */
function AnchorChip({ point }: { point: MeasurePoint | undefined }) {
  if (!point) return <span>—</span>;
  if (!point.target) {
    // Coordinates rather than the word "map point": an unsnapped end is only
    // identifiable by where it is. In --text-secondary, not --text-tertiary,
    // which is 2.36:1 on this background — decorative-only contrast, and this
    // is text the reader has to parse.
    return (
      <span
        style={{
          fontFamily: "var(--font-mono)",
          color: "var(--text-secondary)",
          whiteSpace: "nowrap",
        }}
      >
        {formatLatLng(point.position[0], point.position[1])}
      </span>
    );
  }
  return (
    <span
      style={{
        display: "flex",
        alignItems: "center",
        gap: 4,
        // `minWidth: 0` so MiddleTruncate has something to shrink against;
        // no flex sizing, since each endpoint now owns its own row.
        minWidth: 0,
      }}
    >
      <TypeBadge type={point.target.type} />
      {/* Middle-truncated, not wrapped: real ids share long prefixes, so the
          tail is the discriminating part, and letting a pair of long ids wrap
          onto two lines made the popover jump height as you measured. */}
      <span
        style={{
          fontFamily: "var(--font-mono)",
          color: "var(--text-secondary)",
          minWidth: 0,
          overflow: "hidden",
        }}
      >
        <MiddleTruncate text={point.target.id} />
      </span>
    </span>
  );
}

/**
 * Measure readout, anchored under the measure tool button.
 *
 * Replaces the floating box that used to sit at the bottom of the canvas, where
 * it competed with the legend for the same corner and overlapped it outright on
 * narrow windows. Living under its own button also ties the readout to the mode
 * that produced it, so there is nothing floating over the canvas to explain.
 */
export function MeasurePopover({
  points,
  distanceM,
  onExit,
}: {
  points: readonly MeasurePoint[];
  distanceM: number | null;
  onExit: () => void;
}) {
  const sys = useUnitSystem();

  const sysLabel = formatDistance(distanceM ?? 0, sys);
  const complete = points.length >= 2;
  return (
    <div
      className="canvas-toolbar"
      // Centred under the button rather than edge-aligned like the basemap
      // dropdown: the readout is wider than the 30px button, so aligning an
      // edge would hang it lopsidedly off one side.
      //
      // Opaque, unlike the toolbar itself: this is a value to read, not chrome
      // to look past, and `.canvas-toolbar`'s 0.72 alpha left it competing with
      // whatever basemap happened to be underneath.
      style={{
        position: "absolute",
        top: "calc(100% + 6px)",
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 30,
        flexDirection: "column",
        alignItems: "stretch",
        gap: 5,
        padding: "8px 10px",
        minWidth: 190,
        // A bound for MiddleTruncate to shrink long ids against; without one the
        // popover just grows and hangs off the toolbar.
        maxWidth: 250,
        background: "var(--bg-panel)",
        cursor: "default",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 10,
        }}
      >
        <span
          style={{
            fontSize: "var(--text-2xs)",
            fontWeight: 700,
            letterSpacing: "0.06em",
            textTransform: "uppercase",
            color: "#d4a017",
            whiteSpace: "nowrap",
          }}
        >
          Measure
        </span>
        <button
          type="button"
          onClick={onExit}
          aria-label="Exit measure mode"
          data-tooltip="Exit measure (Esc)"
          data-tooltip-pos="bottom"
          style={{
            display: "inline-flex",
            border: "none",
            background: "transparent",
            color: "var(--text-tertiary)",
            cursor: "pointer",
            padding: 0,
          }}
        >
          <XMarkIcon style={{ width: 12, height: 12 }} />
        </button>
      </div>

      {/* Endpoints stack, one per row. Laying them out inline alongside prose
          made each fragment a flex item, and the trailing sentence collapsed to
          one word per line. */}
      {points.length > 0 && (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 3,
            fontSize: "var(--text-sm)",
          }}
        >
          <EndpointRow marker="A" point={points[0]} />
          {complete && <EndpointRow marker="B" point={points[1]} />}
        </div>
      )}

      {complete ? (
        <div
          style={{
            fontSize: "var(--text-2xl)",
            fontWeight: 600,
            fontFamily: "var(--font-mono)",
            color: "#d4a017",
            lineHeight: 1.1,
            whiteSpace: "nowrap",
          }}
        >
          {sysLabel}
        </div>
      ) : (
        <div
          style={{
            fontSize: "var(--text-sm)",
            color: "var(--text-secondary)",
            lineHeight: 1.45,
          }}
        >
          {points.length === 0
            ? "Click a node, a link, or open space to start."
            : "Click a second point."}
        </div>
      )}
    </div>
  );
}

/** One endpoint: an A/B marker and what that end attached to. */
function EndpointRow({
  marker,
  point,
}: {
  marker: "A" | "B";
  point: MeasurePoint | undefined;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        minWidth: 0,
        whiteSpace: "nowrap",
      }}
    >
      <span
        style={{
          fontSize: "var(--text-2xs)",
          fontWeight: 700,
          color: "var(--text-tertiary)",
          fontFamily: "var(--font-mono)",
          width: 8,
          flexShrink: 0,
        }}
      >
        {marker}
      </span>
      <AnchorChip point={point} />
    </div>
  );
}
