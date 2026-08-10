import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { tooltipTextLayout } from "./TooltipPortal";

/**
 * Whether a tooltip is a label or a sentence.
 *
 * Every tooltip used to be `nowrap`, which is right for a label and wrong
 * for an explanation: the legend's motion note is a sentence now that it
 * has stopped being three lines of the panel, and on one line it would run
 * off both edges of the window and be clamped rather than read.
 *
 * The difference is invisible to the other test layers — jsdom measures
 * every box as zero — so this asserts the decision rather than the result.
 */

describe("tooltipTextLayout", () => {
  it("keeps a label on one line", () => {
    for (const label of ["Close", "Basemap", "Edit / move nodes (E)"]) {
      expect(tooltipTextLayout(label)).toEqual({ whiteSpace: "nowrap" });
    }
  });

  it("lets a sentence wrap, within a measure", () => {
    const sentence =
      "Motion follows the water — Flow, Velocity, Status, Unit headloss and " +
      "Quality. Anything else on this map is a still reading.";
    const layout = tooltipTextLayout(sentence);
    expect(layout.whiteSpace).toBe("normal");
    expect(layout.maxWidth).toBeGreaterThan(0);
  });

  it("keeps a criteria summary on one line", () => {
    // The toolbar chip reads its whole standard back on hover, and it is a
    // row of values meant to be scanned across, not prose. It sits just
    // under the threshold on purpose.
    expect(tooltipTextLayout("≥ 14 m · V 0.1–1.5 m/s · Q 0.1–10 L/s")).toEqual({
      whiteSpace: "nowrap",
    });
  });
});

/**
 * One tooltip per control.
 *
 * `data-tooltip` is this app's own, shown after its own delay and styled
 * with the app. `title` is the browser's, shown after the browser's. An
 * element carrying both shows both: ours appears, and a second later the
 * operating system draws its own copy underneath it.
 *
 * The fix is never to delete `title` alone — on an icon-only button it is
 * also the accessible name — so the two offenders became `aria-label`,
 * which names the control without drawing anything.
 */

const SRC = join(import.meta.dirname ?? __dirname, "..", "..");

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) out.push(...sourceFiles(path));
    else if (name.endsWith(".tsx") && !name.endsWith(".test.tsx"))
      out.push(path);
  }
  return out;
}

/**
 * Every JSX opening tag in `text`.
 *
 * Depth-aware because attribute values hold arrow functions: a naive scan
 * to the first `>` ends the tag inside `onClick={() => …}` and misses
 * every attribute after it — which is how the pair below survived a
 * search that reported none.
 */
function openingTags(text: string): string[] {
  const tags: string[] = [];
  for (let i = 0; i < text.length; i += 1) {
    if (text[i] !== "<" || !/[A-Za-z]/.test(text[i + 1] ?? "")) continue;
    let depth = 0;
    for (let j = i + 1; j < text.length; j += 1) {
      const ch = text[j];
      if (ch === "{") depth += 1;
      else if (ch === "}") depth -= 1;
      else if (ch === ">" && depth === 0) {
        tags.push(text.slice(i, j + 1));
        i = j;
        break;
      }
    }
  }
  return tags;
}

describe("the app's tooltip and the browser's", () => {
  it("are never on the same element", () => {
    const offenders: string[] = [];
    for (const file of sourceFiles(SRC)) {
      for (const tag of openingTags(readFileSync(file, "utf8"))) {
        if (tag.includes("data-tooltip=") && /\stitle=/.test(tag)) {
          offenders.push(`${file.slice(SRC.length + 1)}: ${tag.slice(0, 60)}…`);
        }
      }
    }
    expect(
      offenders,
      "an element with both draws two tooltips — the app's, then the " +
        "browser's a second later. Use `aria-label` where the `title` was " +
        "naming the control.",
    ).toEqual([]);
  });

  it("finds attributes written after an arrow function", () => {
    // The parser's own guard: the pair that prompted this sat below an
    // `onClick={(e) => …}`, and a scan that stopped at the first `>`
    // reported a clean tree.
    const tags = openingTags('<button onClick={() => f(a > b)} title="x" />');
    expect(tags).toHaveLength(1);
    expect(tags[0]).toContain('title="x"');
  });
});
