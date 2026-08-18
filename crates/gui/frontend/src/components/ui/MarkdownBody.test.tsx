// @vitest-environment jsdom
/**
 * What the release-notes and licence documents actually render.
 *
 * Two claims are pinned here, and both were found by chasing CodeQL alert
 * js/incomplete-multi-character-sanitization on `stripHtmlComments`:
 *
 * 1. Raw HTML is inert. That is what makes the alert a false positive: a
 *    leftover `<!--` cannot become an element, because this renderer is
 *    configured without `rehype-raw` and injected markup is dropped rather
 *    than built. Adding raw-HTML support would fail this test, which is the
 *    point of writing it down.
 * 2. An unterminated comment does not blank the document. CommonMark runs an
 *    unclosed comment to the end of input, so before `stripHtmlComments`
 *    removed it, one stray `<!--` in a release body silently discarded every
 *    line after it. useReleaseNotes.test.ts holds the sanitising half.
 */
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { stripHtmlComments } from "../../hooks/useReleaseNotes";
import { MarkdownBody } from "./MarkdownBody";

const renderMd = (md: string) => render(<MarkdownBody>{md}</MarkdownBody>);

describe("MarkdownBody", () => {
  it("renders ordinary markdown", () => {
    const { container } = renderMd("# A real heading\n\nsome text");
    expect(container.textContent).toContain("A real heading");
    expect(container.textContent).toContain("some text");
  });

  it("builds no elements from injected markup, and runs nothing", () => {
    const { container } = renderMd(
      [
        '<img src="x" onerror="window.__pwned = true">',
        "<script>window.__pwned = true</script>",
        "<b>bold?</b>",
      ].join("\n\n"),
    );
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelector("script")).toBeNull();
    expect(container.querySelector("b")).toBeNull();
    expect(
      (window as unknown as Record<string, unknown>).__pwned,
    ).toBeUndefined();
  });

  it("keeps the notes readable when a comment is never closed", () => {
    const body =
      "<!-- someone forgot to close this\n\n# Release 9.9.9\n\n- a fix";
    // Unsanitised, CommonMark swallows the whole document.
    expect(renderMd(body).container.textContent).toBe("");
    // Sanitised at the data boundary, which is how the app renders it.
    const shown = renderMd(stripHtmlComments(body)).container.textContent ?? "";
    expect(shown).toContain("Release 9.9.9");
    expect(shown).toContain("a fix");
  });
});
