/**
 * Handing deck a camera it will actually take.
 *
 * deck owns the schematic camera: `initialViewState` is given once and the
 * camera is moved afterwards by handing deck a new one. What is easy to
 * miss is that deck takes the new one only when it *differs*. `setProps`
 * compares the incoming `initialViewState` against the previous prop value
 * and overwrites its internal camera only if they are unequal three levels
 * deep — `@deck.gl/core`, `lib/deck.js`, at the comment "Overwrite internal
 * view state".
 *
 * The previous prop is not where the camera is. A gesture moves deck's
 * internal camera and leaves the prop exactly as it was, so the second of
 * two identical writes is dropped: fly to an element, pan away, ask for it
 * again — nothing happens. The same silence swallows a repeated fit and a
 * zoom step that lands where the last one did. It fails only after a
 * gesture, which is the hardest kind of nothing to reproduce on purpose.
 *
 * A fresh interpolator is what keeps each write distinct. deck compares
 * props to a depth of three and an interpolator's own options sit below
 * that, so two separately constructed ones never compare equal — meaning
 * every camera built here differs from the last whatever its numbers say.
 *
 * The instant case carries one too, and declares itself with a zero
 * duration. deck starts a transition only for a positive duration, and a
 * write that starts none cancels whatever was in flight — which is what an
 * interrupting move should do regardless.
 *
 * The axis zooms are the other thing that has to be here. An orthographic
 * camera keeps `zoom` beside a `zoomX` and a `zoomY`, and where all three
 * are present the axis pair wins — `normalizeZoom` in deck's
 * `orthographic-controller.js` reads `zoom` only as their fallback. A
 * transition merges each interpolated frame over the *previous frame's*
 * viewport props, which carry that pair, so interpolating `zoom` alone
 * moves a number nothing reads: the flight pans while the zoom sits at
 * wherever the first frame left it, then snaps home at the end. Hydra never
 * zooms the axes apart, so all three are written and all three travel.
 */

import { LinearInterpolator } from "@deck.gl/core";
import type { OrthoCamera } from "./flyToCamera";

/** An `initialViewState` for deck, with its transition settings. */
export interface DeckCamera extends OrthoCamera {
  zoomX: number;
  zoomY: number;
  transitionDuration: number;
  transitionInterpolator: LinearInterpolator;
}

/** What travels during a flight. */
const FLOWN_PROPS = ["target", "zoom", "zoomX", "zoomY"];

/**
 * What a camera must have before one can start.
 *
 * The axis zooms are interpolated when present but not demanded, so a
 * camera arriving from somewhere that only knows about `zoom` still passes
 * deck's check rather than throwing mid-flight.
 */
const REQUIRED_PROPS = ["target", "zoom"];

/**
 * The camera to hand deck, flown over `durationMs` or arriving at once.
 *
 * Every programmatic move goes through this — the framing pass, the fit,
 * the fly-to and the zoom buttons — so none of them can be the one that
 * forgets. How long each of those travels is `cameraMotion`'s to say, not
 * this module's: a fit and a flight to one element are different journeys
 * and want different times.
 */
export function deckCamera(to: OrthoCamera, durationMs = 0): DeckCamera {
  return {
    ...to,
    // Written on every camera, not only the flown ones: these are what the
    // *next* flight starts from, and one that begins without them
    // interpolates its zoom up from nothing.
    zoomX: to.zoom,
    zoomY: to.zoom,
    transitionDuration: durationMs,
    // Linear: an orthographic camera has no horizon to arc over, and a
    // flight curve would read as a wobble.
    //
    // Built fresh here rather than shared from a constant. deck's
    // comparison bottoms out at exactly this nesting, so one shared options
    // object would make every camera compare equal to the last and undo the
    // whole reason this module exists. The two tests that ask for the same
    // destination twice are what hold that in place — they caught it being
    // reintroduced once already.
    transitionInterpolator: new LinearInterpolator({
      transitionProps: {
        compare: [...FLOWN_PROPS],
        required: [...REQUIRED_PROPS],
      },
    }),
  };
}
