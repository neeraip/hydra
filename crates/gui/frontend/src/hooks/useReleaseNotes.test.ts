import { describe, expect, it } from "vitest";
import {
  cleanReleaseBody,
  compareSemver,
  defaultExpandedVersions,
  type GuiRelease,
  guiVersionFromTag,
  releaseHasNotes,
  releasesWithContent,
  sortReleasesDesc,
  stripChangelogLines,
  stripHtmlComments,
  stripTrailingInstallationNote,
  unseenReleases,
} from "./useReleaseNotes";

function rel(version: string): GuiRelease {
  return { version, date: "", body: "", releaseUrl: "" };
}

describe("guiVersionFromTag", () => {
  it("accepts only the GUI release track", () => {
    expect(guiVersionFromTag("gui-v2.1.0")).toBe("2.1.0");
    expect(guiVersionFromTag("gui-v10.0.3")).toBe("10.0.3");
  });

  it("rejects library, CLI, and malformed tags", () => {
    expect(guiVersionFromTag("v2.1.0")).toBeNull();
    expect(guiVersionFromTag("cli-v2.1.0")).toBeNull();
    expect(guiVersionFromTag("gui-v")).toBeNull();
    expect(guiVersionFromTag("gui-2.1.0")).toBeNull();
    expect(guiVersionFromTag("")).toBeNull();
  });
});

describe("compareSemver", () => {
  it("orders numerically per dot segment", () => {
    expect(compareSemver("2.1.0", "2.0.9")).toBe(1);
    expect(compareSemver("2.0.9", "2.1.0")).toBe(-1);
    expect(compareSemver("2.1.0", "2.1.0")).toBe(0);
    // Numeric, not lexicographic: 10 > 9.
    expect(compareSemver("2.10.0", "2.9.0")).toBe(1);
  });

  it("treats missing segments as zero", () => {
    expect(compareSemver("2.1", "2.1.0")).toBe(0);
    expect(compareSemver("2", "2.0.1")).toBe(-1);
  });
});

describe("sortReleasesDesc", () => {
  it("sorts newest first without mutating the input", () => {
    const input = [rel("2.0.0"), rel("2.10.0"), rel("2.2.0")];
    const sorted = sortReleasesDesc(input);
    expect(sorted.map((r) => r.version)).toEqual(["2.10.0", "2.2.0", "2.0.0"]);
    expect(input.map((r) => r.version)).toEqual(["2.0.0", "2.10.0", "2.2.0"]);
  });
});

describe("unseenReleases", () => {
  const releases = [rel("2.2.0"), rel("2.1.0"), rel("2.0.0")];

  it("treats an unseeded (null) marker as nothing unseen", () => {
    // First-run seeding hasn't resolved the app version yet — no backlog
    // flash while it does.
    expect(unseenReleases(releases, null)).toEqual([]);
  });

  it("returns only releases strictly newer than the marker", () => {
    expect(unseenReleases(releases, "2.0.0").map((r) => r.version)).toEqual([
      "2.2.0",
      "2.1.0",
    ]);
    expect(unseenReleases(releases, "2.1.0").map((r) => r.version)).toEqual([
      "2.2.0",
    ]);
  });

  it("returns [] when the marker matches or exceeds the newest", () => {
    expect(unseenReleases(releases, "2.2.0")).toEqual([]);
    expect(unseenReleases(releases, "3.0.0")).toEqual([]);
  });
});

describe("stripHtmlComments", () => {
  it("removes GitHub's generated preamble comment, keeping the markdown", () => {
    const body =
      "<!-- Release notes generated using configuration in .github/release.yml at gui-v2.1.0 -->\n\n## What's Changed\n- Fix things";
    expect(stripHtmlComments(body)).toBe("## What's Changed\n- Fix things");
  });

  it("removes multiple and multi-line comments", () => {
    expect(stripHtmlComments("a <!-- one -->b<!-- two\nlines -->c")).toBe(
      "a bc",
    );
  });

  it("reduces a comment-only body to the empty string (empty-body case)", () => {
    expect(stripHtmlComments("<!-- only a comment -->")).toBe("");
    expect(stripHtmlComments("  <!-- c1 -->\n<!-- c2 -->  ")).toBe("");
  });

  it("leaves comment-free bodies untouched apart from trimming", () => {
    expect(stripHtmlComments("  ## Notes\n- item  ")).toBe("## Notes\n- item");
  });

  // An unterminated comment used to survive both this function and the
  // renderer: CommonMark runs an unclosed comment to the end of the
  // document, so react-markdown dropped every line after it and the notes
  // came out blank. MarkdownBody.test.tsx holds the rendering half.
  it("removes an unterminated comment, keeping nothing after it hostage", () => {
    expect(stripHtmlComments("<!-- oops\n\n## Notes\n- item")).toBe(
      "## Notes\n- item",
    );
    expect(stripHtmlComments("## Notes\n\n<!-- trailing note")).toBe(
      "## Notes",
    );
  });

  it("never leaves an opening token behind", () => {
    for (const body of [
      "<!-- oops",
      "<!--",
      "text <!-- dangling",
      "<!-- closed --> then <!-- dangling",
      "<!--<!---->-->",
    ]) {
      expect(stripHtmlComments(body)).not.toContain("<!--");
    }
  });
});

describe("stripChangelogLines", () => {
  const url =
    "https://github.com/neeraip/hydra/compare/gui-v2.0.0...gui-v2.1.0";

  it("strips whole changelog lines in bold, plain, and colon-less variants", () => {
    expect(
      stripChangelogLines(`## Notes\n- item\n\n**Full Changelog**: ${url}`),
    ).toBe("## Notes\n- item");
    expect(
      stripChangelogLines(`## Notes\n- item\n\nFull Changelog: ${url}`),
    ).toBe("## Notes\n- item");
    expect(
      stripChangelogLines(`## Notes\n- item\n\nfull changelog ${url}`),
    ).toBe("## Notes\n- item");
  });

  it("strips the line anywhere, including BEFORE the notes", () => {
    expect(
      stripChangelogLines(`**Full Changelog**: ${url}\n\n## Notes\n- item`),
    ).toBe("## Notes\n- item");
    expect(stripChangelogLines(`## A\n**Full Changelog**: ${url}\n## B`)).toBe(
      "## A\n## B",
    );
  });

  it("is not defeated by CRLF endings or trailing whitespace", () => {
    expect(
      stripChangelogLines(
        `## Notes\r\n- item\r\n\r\n**Full Changelog**: ${url}  \r\n`,
      ),
    ).toBe("## Notes\n- item");
    expect(stripChangelogLines(`**Full Changelog**: ${url}\r\n`)).toBe("");
  });

  it("preserves prose mentions (whole-line anchoring)", () => {
    const mid = `See the **Full Changelog**: ${url} for details.\n\n- item`;
    expect(stripChangelogLines(mid)).toBe(mid);
    const prose = "## Notes\n\nsee the full changelog for details";
    expect(stripChangelogLines(prose)).toBe(prose);
  });

  it("reduces a changelog-only body to the empty string", () => {
    expect(stripChangelogLines(`**Full Changelog**: ${url}`)).toBe("");
  });

  it("degrades to leaving unconventional lines visible", () => {
    const body = "## Notes\n\nFull Changelog is available on request";
    expect(stripChangelogLines(body)).toBe(body);
  });
});

describe("cleanReleaseBody", () => {
  it("empties a body that is only comment + changelog plumbing", () => {
    expect(
      cleanReleaseBody(
        "<!-- Release notes generated using configuration -->\n\n**Full Changelog**: https://github.com/x/compare/a...b",
      ),
    ).toBe("");
  });

  it("keeps real content while removing both kinds of plumbing", () => {
    expect(
      cleanReleaseBody(
        "<!-- gen -->\n## What's Changed\n- Fix\n\n**Full Changelog**: https://github.com/x/compare/a...b",
      ),
    ).toBe("## What's Changed\n- Fix");
  });
});

describe("releasesWithContent", () => {
  const withBody = { ...rel("2.2.0"), body: "## Notes" };
  const plumbingOnly = rel("2.1.0"); // cleaned body is empty
  const whitespace = { ...rel("2.0.0"), body: "  \n " };

  it("excludes cleaned-empty releases from the earlier-updates count", () => {
    // Plumbing-only releases still appear in the modal stack (as compact
    // rows) — this filter only keeps the teaser count honest.
    expect(
      releasesWithContent([withBody, plumbingOnly, whitespace]).map(
        (r) => r.version,
      ),
    ).toEqual(["2.2.0"]);
  });

  it("returns [] when every release is pure plumbing (count shows nothing)", () => {
    expect(releasesWithContent([plumbingOnly, whitespace])).toEqual([]);
  });
});

describe("releaseHasNotes", () => {
  it("drives the explicit empty state for cleaned-empty bodies", () => {
    expect(releaseHasNotes({ ...rel("2.1.0"), body: "## Notes" })).toBe(true);
    expect(releaseHasNotes(rel("2.1.0"))).toBe(false);
    expect(releaseHasNotes({ ...rel("2.1.0"), body: " \n " })).toBe(false);
  });
});

describe("stripTrailingInstallationNote", () => {
  const boilerplate =
    "---\n\n## Installation Note\n\nThese installers are currently **unsigned**.\n\n**macOS**\n1. Open **Terminal**";

  it("cleans the real gui-v1.1.0 shape (comment + changelog + note, CRLF) to empty", () => {
    const body =
      "<!-- Release notes generated using configuration in .github/release.yml at gui-v1.1.0 -->\r\n\r\n**Full Changelog**: https://github.com/neeraip/hydra/compare/gui-v1.0.2...gui-v1.1.0\r\n\r\n\r\n---\r\n\r\n## Installation Note\r\n\r\nThese installers are currently **unsigned**. You may need extra steps to run the app.\r\n\r\n**macOS**\r\n1. Open **Terminal**";
    expect(cleanReleaseBody(body)).toBe("");
  });

  it("drops the boilerplate while keeping real notes before it", () => {
    expect(
      stripTrailingInstallationNote(
        `## What's Changed\n- Fix\n\n${boilerplate}`,
      ),
    ).toBe("## What's Changed\n- Fix");
  });

  it("strips heading-level variants", () => {
    expect(
      stripTrailingInstallationNote(
        "## Notes\n\n---\n\n### installation note\n\nsteps",
      ),
    ).toBe("## Notes");
  });

  it("leaves a break NOT followed by the heading untouched", () => {
    const body = "## Notes\n\n---\n\n## Something Else\n\ntext";
    expect(stripTrailingInstallationNote(body)).toBe(body);
  });

  it("leaves a mid-body installation note without a preceding break untouched", () => {
    const body = "## Notes\n\n## Installation Note\n\nsteps\n\n- more notes";
    expect(stripTrailingInstallationNote(body)).toBe(body);
  });
});

describe("defaultExpandedVersions", () => {
  const newest = { ...rel("2.2.0"), body: "## Notes" };
  const middle = { ...rel("2.1.0"), body: "## Notes" };
  const oldest = { ...rel("2.0.0"), body: "## Notes" };

  it("expands unseen releases plus the newest even when already seen", () => {
    // Marker at newest: nothing unseen, but the newest still starts open.
    expect(defaultExpandedVersions([newest, middle, oldest], "2.2.0")).toEqual(
      new Set(["2.2.0"]),
    );
    // Marker mid-history: unseen (2.2.0, 2.1.0) open, seen older collapsed.
    expect(defaultExpandedVersions([newest, middle, oldest], "2.0.0")).toEqual(
      new Set(["2.2.0", "2.1.0"]),
    );
  });

  it("expands only the newest when the marker is unseeded (null)", () => {
    expect(defaultExpandedVersions([newest, middle], null)).toEqual(
      new Set(["2.2.0"]),
    );
  });

  it("never includes notes-less releases (compact rows have nothing to expand)", () => {
    const emptyNewest = rel("2.3.0"); // cleaned body empty
    expect(
      defaultExpandedVersions([emptyNewest, newest, middle], "2.1.0"),
    ).toEqual(new Set(["2.2.0"]));
  });

  it("returns an empty set for no releases", () => {
    expect(defaultExpandedVersions([], "1.0.0")).toEqual(new Set());
  });
});
