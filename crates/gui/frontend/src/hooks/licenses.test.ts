import { describe, expect, it } from "vitest";
import {
  matchesComponentQuery,
  noticeFallback,
  repoLink,
  type ThirdPartyComponent,
} from "./licenses";

function component(
  over: Partial<ThirdPartyComponent> = {},
): ThirdPartyComponent {
  return {
    name: "serde",
    version: "1.0.219",
    ecosystem: "rust",
    spdx: "MIT OR Apache-2.0",
    url: "https://github.com/serde-rs/serde",
    files: [{ name: "LICENSE-MIT", text: 3 }],
    ...over,
  };
}

describe("matchesComponentQuery", () => {
  it("admits everything when nothing is typed", () => {
    expect(matchesComponentQuery(component(), "")).toBe(true);
    expect(matchesComponentQuery(component(), "   ")).toBe(true);
  });

  it("matches on name, ignoring case and surrounding space", () => {
    expect(matchesComponentQuery(component(), "SER")).toBe(true);
    expect(matchesComponentQuery(component(), "  serde ")).toBe(true);
    expect(matchesComponentQuery(component(), "tokio")).toBe(false);
  });

  it("matches on the version, which is how an advisory names a package", () => {
    expect(matchesComponentQuery(component(), "1.0.219")).toBe(true);
    expect(matchesComponentQuery(component(), "1.0.220")).toBe(false);
  });

  it("matches on the licence and the ecosystem", () => {
    expect(matchesComponentQuery(component(), "apache")).toBe(true);
    expect(matchesComponentQuery(component(), "npm")).toBe(false);
    expect(matchesComponentQuery(component({ ecosystem: "npm" }), "npm")).toBe(
      true,
    );
  });
});

describe("noticeFallback", () => {
  it("says nothing when the package ships its own text", () => {
    expect(noticeFallback(component())).toBeNull();
  });

  it("names the licence and where to find the package", () => {
    const note = noticeFallback(component({ files: [] }));
    expect(note).toContain("MIT OR Apache-2.0");
    expect(note).toContain("https://github.com/serde-rs/serde");
  });

  it("omits a home the package never stated", () => {
    const note = noticeFallback(component({ files: [], url: "" }));
    expect(note).toContain("MIT OR Apache-2.0");
    expect(note).not.toContain("see ");
  });

  it("still renders something for a package that declares nothing", () => {
    // Guarded on the Rust side too, so this is the belt to that braces —
    // but a row with no text, no licence and no note would be a blank
    // entry a reader could not act on.
    expect(noticeFallback(component({ files: [], spdx: "", url: "" }))).toBe(
      "No licence declared. This package ships no licence file.",
    );
  });
});

describe("repoLink", () => {
  const source = "https://github.com/neeraip/hydra";

  it("resolves a sibling file onto the published source", () => {
    expect(repoLink("LICENSE", source)).toBe(`${source}/blob/main/LICENSE`);
    expect(repoLink("./docs/src/license.md", source)).toBe(
      `${source}/blob/main/docs/src/license.md`,
    );
    expect(repoLink("/AGENTS.md", source)).toBe(
      `${source}/blob/main/AGENTS.md`,
    );
  });

  it("leaves alone anything that already points somewhere", () => {
    expect(repoLink("https://example.com/x", source)).toBe(
      "https://example.com/x",
    );
    expect(repoLink("mailto:hello@neer.ai", source)).toBe(
      "mailto:hello@neer.ai",
    );
    expect(repoLink("//cdn.example.com/x", source)).toBe("//cdn.example.com/x");
  });

  it("leaves an in-document anchor alone", () => {
    // Rewriting it would send a reader out to a browser for a heading that
    // is already on screen.
    expect(repoLink("#do-you-need-one", source)).toBe("#do-you-need-one");
  });

  it("tolerates a trailing slash on the source", () => {
    expect(repoLink("LICENSE", `${source}/`)).toBe(
      `${source}/blob/main/LICENSE`,
    );
  });
});
