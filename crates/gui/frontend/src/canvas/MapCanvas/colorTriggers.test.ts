import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/**
 * The list deck makes us keep, checked against the code it describes.
 *
 * deck compiles a per-element accessor into a GPU buffer and reuses it
 * until something in that layer's `updateTriggers` changes. An accessor is
 * a closure, so deck cannot know what it captured — which means the same
 * dependency information React already wants in a hook array has to be
 * written a second time, in a second syntax, with nothing checking it.
 *
 * Three defects this session were that list drifting: a node radius trigger
 * left over from when the radius was a constant, a flow trigger that did
 * not name the variable when the pulse gained variables, and a grid colour
 * with no trigger at all. Every one presented the same way — right value
 * computed, handed to deck, ignored — which reads as "works after a
 * reload".
 *
 * Biome lints React's array. Nothing lints deck's, so this does: it reads
 * the source, finds what the colour accessors actually close over, and
 * requires each of those to appear in the trigger list beside them.
 *
 * Deliberately narrow. It covers the two accessors whose inputs are many
 * and keep growing; geometry accessors read only their datum, and deck
 * invalidates those itself when `data` changes.
 */

const SOURCE = readFileSync(
  fileURLToPath(new URL("../MapCanvas.tsx", import.meta.url)),
  "utf8",
);

/**
 * The body of a `const <name> = (…) => {…}` accessor.
 *
 * From the opening brace, so the signature is left out: a parameter's type
 * annotation names the layer's data — `(d: (typeof linkData)[number])` —
 * and that is not a capture. deck rebuilds a layer's attributes when its
 * `data` changes identity, without being told.
 */
function accessorBody(name: string): string {
  const start = SOURCE.indexOf(`const ${name} = (`);
  expect(start, `${name} not found`).toBeGreaterThan(-1);
  const open = SOURCE.indexOf("{", start);
  let depth = 0;
  for (let i = open; i < SOURCE.length; i += 1) {
    const c = SOURCE[i];
    if (c === "{") depth += 1;
    else if (c === "}") {
      depth -= 1;
      if (depth === 0) return SOURCE.slice(open, i + 1);
    }
  }
  throw new Error(`unterminated ${name}`);
}

/** The contents of a bracketed trigger list, by the text that precedes it. */
function triggerList(anchor: string): string {
  const at = SOURCE.indexOf(anchor);
  expect(at, `${anchor} not found`).toBeGreaterThan(-1);
  const open = SOURCE.indexOf("[", at);
  let depth = 0;
  for (let i = open; i < SOURCE.length; i += 1) {
    if (SOURCE[i] === "[") depth += 1;
    else if (SOURCE[i] === "]") {
      depth -= 1;
      if (depth === 0) return SOURCE.slice(open, i + 1);
    }
  }
  throw new Error(`unterminated list after ${anchor}`);
}

/**
 * Identifiers the accessor reads from outside itself.
 *
 * Its own parameter and locals are excluded, as are the helpers it calls —
 * a function's identity does not change with the data, and listing every
 * imported helper would make the list noise. What remains is the reactive
 * state the closure captured, which is exactly what deck needs told.
 */
function captured(body: string, ignore: readonly string[]): string[] {
  // Comments first, or every word of the prose explaining the accessor
  // arrives as an identifier.
  const code = body
    .replace(/\/\*[\s\S]*?\*\//g, " ")
    .replace(/\/\/[^\n]*/g, " ");
  const locals = new Set<string>([
    ...ignore,
    ...Array.from(code.matchAll(/\bconst\s+(\w+)/g), (m) => m[1]),
  ]);
  const found = new Set<string>();
  for (const [, id] of code.matchAll(/\b([a-z][A-Za-z0-9]*)\b(?!\s*[:(])/g)) {
    if (!locals.has(id)) found.add(id);
  }
  return [...found].sort();
}

/** Language, not state. */
const KEYWORDS = [
  "as",
  "any",
  "const",
  "else",
  "if",
  "number",
  "return",
  "typeof",
  "undefined",
];

/** The datum and what is read off it: per-element, not captured state. */
const PER_DATUM = ["d", "role", "si", "type", "values", "variable"];

/** Class names passed to the shared ramp helpers. */
const LITERALS = ["polyline", "point", "status"];

/**
 * Derived inside the layer builder from values the trigger list does name.
 *
 * Each is `colorMode === "threshold" ? <thresholds> : undefined`, so the
 * list carrying `colorMode` and the threshold objects already covers it.
 * Named here rather than silently skipped, because an exception nobody can
 * see is how the list drifts in the first place.
 */
const DERIVED_FROM_LISTED = ["velThresh", "flowThresh", "pressThresh"];

const NOT_STATE = [
  ...KEYWORDS,
  ...PER_DATUM,
  ...LITERALS,
  ...DERIVED_FROM_LISTED,
];

describe("the link colour trigger", () => {
  /**
   * The load-bearing one. A value the accessor reads and the list omits is
   * a colour that stops updating, and nothing else in the toolchain will
   * say so.
   */
  it("names everything the accessor reads", () => {
    const body = accessorBody("linkColor");
    const list = triggerList("const linkColorTriggers =");
    for (const id of captured(body, NOT_STATE)) {
      expect(
        list,
        `linkColor reads \`${id}\`, trigger list does not`,
      ).toContain(id);
    }
  });
});

describe("the node colour trigger", () => {
  it("names everything the accessor reads", () => {
    const body = accessorBody("nodeColor");
    const list = triggerList("// Everything `nodeColor` reads");
    for (const id of captured(body, NOT_STATE)) {
      expect(
        list,
        `nodeColor reads \`${id}\`, trigger list does not`,
      ).toContain(id);
    }
  });
});
