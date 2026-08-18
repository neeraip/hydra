// @vitest-environment jsdom
import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import {
  clearElementBadges,
  registerElementBadges,
} from "../../../types/elementTypes";
import { Header } from "./InspectorHeader";

/**
 * The inspector header is where an element says what it is, and the
 * interface rule is that a kind is shown by its glyph, never by its name
 * alone. This header used to show the glyph *and* a second mark that
 * each caller drew for itself: a dot for a node, a stripe for a link, an
 * outlined box for a region. Every one was the kind's colour and nothing
 * more, so it looked like it carried the same information as the badge
 * while carrying less, and each was free to drift from the catalog on
 * its own.
 *
 * What stops those coming back is the type signature, not this file: the
 * `badge` prop is gone, so a caller offering one fails to compile. What
 * is pinned here is the other half, which no compiler can check, that
 * the header does show the glyph and shows it once.
 */

beforeEach(() => {
  clearElementBadges();
});

function renderHeader(subtitle: string) {
  return render(
    <Header
      id="J9"
      subtitle={subtitle}
      accentColor="#0af"
      onClose={() => {}}
    />,
  );
}

describe("inspector header", () => {
  it("shows the kind's glyph, not the name on its own", () => {
    registerElementBadges([{ id: "junction", badge: "J" }]);
    renderHeader("junction");
    expect(screen.getByText("J")).toBeTruthy();
  });

  it("shows the name beside the glyph, which the rule allows", () => {
    registerElementBadges([{ id: "junction", badge: "J" }]);
    renderHeader("junction");
    expect(screen.getByText("junction")).toBeTruthy();
  });

  it("shows the element's id", () => {
    renderHeader("junction");
    expect(screen.getByText("J9")).toBeTruthy();
  });

  it("marks the kind exactly once", () => {
    // The badge names the kind in its tooltip. A second mark of kind
    // would either duplicate that or, as the hand-rolled ones did,
    // appear with no tooltip at all beside it.
    registerElementBadges([{ id: "conduit", badge: "CO" }]);
    const { container } = renderHeader("conduit");
    expect(container.querySelectorAll('[data-tooltip="Conduit"]')).toHaveLength(
      1,
    );
  });

  it("takes the engine's own letters over this layer's guess", () => {
    // Six drainage kinds once fell through to their initial, so
    // `landuse` and `lidcontrol` both rendered as "L".
    registerElementBadges([{ id: "lidcontrol", badge: "LID" }]);
    renderHeader("lidcontrol");
    expect(screen.getByText("LID")).toBeTruthy();
  });
});
