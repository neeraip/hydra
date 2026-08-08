import { OrthographicViewport } from "@deck.gl/core";
import { describe, expect, it } from "vitest";
import { FIT_DURATION_MS, FLY_DURATION_MS } from "../cameraMotion";
import { deckCamera } from "./deckCamera";
import type { OrthoCamera } from "./flyToCamera";

/**
 * deck's own comparison, copied.
 *
 * The rule this module exists for lives in a dependency — `@deck.gl/core`,
 * `lib/deck.js`: a new `initialViewState` overwrites deck's camera only
 * when `deepEqual(previousProp, nextProp, 3)` says they differ. There is no
 * way to ask deck that question from here, so it is asked of a copy, which
 * is the same arrangement the Rust DTOs and their TypeScript mirrors have:
 * a claim that matters on both sides is asserted on both sides.
 *
 * Transcribed from `@deck.gl/core/dist/utils/deep-equal.js`. If deck ever
 * loosens it, these tests keep passing and the app breaks — so what they
 * really guard is that *our* side keeps producing distinguishable cameras,
 * which is the half we control.
 */
function deckDeepEqual(a: unknown, b: unknown, depth: number): boolean {
  if (a === b) return true;
  if (!depth || !a || !b) return false;
  if (Array.isArray(a)) {
    if (!Array.isArray(b) || a.length !== b.length) return false;
    return a.every((v, i) => deckDeepEqual(v, b[i], depth - 1));
  }
  if (Array.isArray(b)) return false;
  if (typeof a === "object" && typeof b === "object") {
    const aKeys = Object.keys(a);
    const bKeys = Object.keys(b);
    if (aKeys.length !== bKeys.length) return false;
    return aKeys.every(
      (k) =>
        bKeys.includes(k) &&
        deckDeepEqual(
          (a as Record<string, unknown>)[k],
          (b as Record<string, unknown>)[k],
          depth - 1,
        ),
    );
  }
  return false;
}

/** deck's question, in the words the caller cares about. */
const deckWouldMove = (a: unknown, b: unknown) => !deckDeepEqual(a, b, 3);

const HERE: OrthoCamera = { target: [120, -40, 0], zoom: 3.5 };
const THERE: OrthoCamera = { target: [900, 900, 0], zoom: 1 };

describe("asking deck to move somewhere it has already been", () => {
  /**
   * The defect this exists for: the camera is moved to a place, a gesture
   * takes it elsewhere without touching the prop, and the same place is
   * asked for again. A plain `{target, zoom}` would compare equal to the
   * prop deck still holds, and the move would be dropped in silence.
   */
  it("moves, even when the destination has not changed", () => {
    expect(deckWouldMove(deckCamera(HERE), deckCamera(HERE))).toBe(true);
  });

  it("moves for a flight to an unchanged destination too", () => {
    expect(
      deckWouldMove(
        deckCamera(HERE, FIT_DURATION_MS),
        deckCamera(HERE, FIT_DURATION_MS),
      ),
    ).toBe(true);
  });

  /** The bare camera is what used to be sent, and is why this module is here. */
  it("is what a bare camera would have failed to do", () => {
    expect(deckWouldMove(HERE, { ...HERE })).toBe(false);
  });

  it("moves between two different destinations, which was never in doubt", () => {
    expect(deckWouldMove(deckCamera(HERE), deckCamera(THERE))).toBe(true);
  });
});

describe("what the camera asks deck to do", () => {
  it("carries the destination through unchanged", () => {
    const camera = deckCamera(HERE, FIT_DURATION_MS);
    expect(camera.target).toEqual(HERE.target);
    expect(camera.zoom).toBe(HERE.zoom);
  });

  /**
   * However long it is given, not a time of its own. A fit and a flight to
   * one element travel for different lengths, and the module that decides
   * that is `cameraMotion`; this one only carries the answer.
   */
  it("flies for exactly as long as it is told to", () => {
    expect(deckCamera(HERE, FIT_DURATION_MS).transitionDuration).toBe(
      FIT_DURATION_MS,
    );
    expect(deckCamera(HERE, FLY_DURATION_MS).transitionDuration).toBe(
      FLY_DURATION_MS,
    );
    expect(FIT_DURATION_MS).not.toBe(FLY_DURATION_MS);
  });

  /**
   * Zero rather than no duration: deck runs a transition only for a
   * positive one, and a write carrying none of the transition props would
   * be a differently-shaped object — which is the thing this must not be.
   */
  it("arrives at once otherwise, and still says so with an interpolator", () => {
    const camera = deckCamera(HERE);
    expect(camera.transitionDuration).toBe(0);
    expect(camera.transitionInterpolator).toBeDefined();
  });

  /** Same shape either way, so an instant move can interrupt a flight. */
  it("has the same keys whether it flies or not", () => {
    expect(Object.keys(deckCamera(HERE)).sort()).toEqual(
      Object.keys(deckCamera(HERE, FIT_DURATION_MS)).sort(),
    );
  });

  it("interpolates the target and every spelling of the zoom", () => {
    const { transitionInterpolator } = deckCamera(HERE, FLY_DURATION_MS);
    const flown = JSON.stringify(transitionInterpolator.opts);
    for (const prop of ["target", "zoom", "zoomX", "zoomY"]) {
      expect(flown).toContain(prop);
    }
  });
});

/**
 * Flown frame by frame through the viewport deck renders with.
 *
 * The defect this covers was invisible to any check on the camera's shape.
 * The numbers were right, the interpolator ran, and `zoom` moved from one
 * end to the other — it just moved somewhere nothing read. An orthographic
 * camera keeps `zoomX`/`zoomY` beside `zoom` and prefers the pair, and a
 * transition merges each interpolated frame over the *previous frame's*
 * viewport props, which carry that pair. So the flight panned while the
 * zoom sat at wherever frame one left it, then snapped home at the end.
 *
 * Nothing short of stepping the frames shows that, so this steps them,
 * against `OrthographicViewport` — deck's own, and the object whose
 * `scale` is what a reader actually sees.
 */
describe("flying, frame by frame", () => {
  const VIEW = { width: 800, height: 600 };

  /** The zoom deck would render at each point along one flight. */
  function zoomsAlong(from: OrthoCamera, to: OrthoCamera): number[] {
    const interpolator = deckCamera(to).transitionInterpolator;
    const rendered = (props: object) => {
      const vp = new OrthographicViewport({ ...VIEW, ...props });
      return {
        target: vp.target,
        zoom: vp.zoom,
        zoomX: vp.zoomX,
        zoomY: vp.zoomY,
      };
    };
    // Raw at the start and rendered at the end, because that is deck's own
    // asymmetry: a flight begins from whatever camera it was last handed —
    // ours, unnormalised — and aims at a destination that has been through
    // a viewport. A camera that leaves the axis zooms out is therefore
    // missing them at exactly the moment a flight reads them.
    const first = { ...VIEW, ...deckCamera(from) };
    const { start, end } = interpolator.initializeProps(
      first,
      rendered(deckCamera(to)),
    );

    // Each frame lands on the one before it, which is what lets a stale
    // axis zoom shadow the one being interpolated.
    let props: Record<string, unknown> = first;
    return [0, 0.25, 0.5, 0.75, 1].map((t) => {
      props = rendered({
        ...props,
        ...interpolator.interpolateProps(start, end, t),
      });
      return props.zoom as number;
    });
  }

  const LOW: OrthoCamera = { target: [0, 0, 0], zoom: 0 };
  const HIGH: OrthoCamera = { target: [400, 0, 0], zoom: 4 };

  it("moves the zoom every frame, not only at the end", () => {
    expect(zoomsAlong(LOW, HIGH)).toEqual([0, 1, 2, 3, 4]);
  });

  /** The symptom, named: held where it started, then home in one jump. */
  it("does not hold at the start and snap", () => {
    expect(zoomsAlong(LOW, HIGH).slice(0, -1)).not.toEqual([0, 0, 0, 0]);
  });

  it("zooms out as readily as in", () => {
    expect(zoomsAlong(HIGH, LOW)).toEqual([4, 3, 2, 1, 0]);
  });
});
