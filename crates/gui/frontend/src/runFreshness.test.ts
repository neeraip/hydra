/**
 * Guards the freshness invariant for run-derived data.
 *
 * An effect that reads from a completed simulation must re-run when a new one
 * lands. The signals for that are `resultGeneration` (SimulationContext's
 * counter, bumped whenever fresh results arrive) and `resultMeta` (replaced on
 * every run, so its identity changes too).
 *
 * Omitting them does not fail loudly: the view keeps whatever it computed
 * before the run and looks perfectly healthy, which is how the Report page
 * spent several releases insisting a freshly simulated project had no results.
 * The stale-but-plausible case is worse — the previous run's numbers, silently.
 *
 * This test is deliberately structural rather than behavioural: it reads the
 * source, because the failure is an absent dependency, and no render test can
 * observe a dependency that was never written.
 */

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const SRC = new URL(".", import.meta.url).pathname;

/** Backend commands whose result is derived from a completed run. */
const RUN_DERIVED_COMMANDS = [
  "load_result_meta",
  "get_period_results",
  "get_pump_energy",
  "get_element_series",
  "get_result_analytics",
  "probe_report_blocks",
  "generate_report",
  "get_run_warnings",
  "get_report_block_options",
];

/** Dependencies that change when a new run lands. */
const FRESHNESS_TOKENS = ["resultGeneration", "resultMeta"];

/** Setters that PRODUCE the freshness signal. An effect that calls one is the
 *  source of the token, not a consumer of it — depending on its own output
 *  would loop forever, so these are exempt. */
const FRESHNESS_PRODUCERS = ["setResultGeneration", "setResultMeta"];

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules") continue;
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (p.endsWith(".tsx") && !p.includes(".test.")) out.push(p);
  }
  return out;
}

/** Exported wrappers around the run-derived commands, discovered from the
 *  hooks rather than hardcoded, so a new wrapper is covered on arrival. */
function runDerivedWrappers(): Set<string> {
  const found = new Set<string>();
  for (const file of [
    "hooks/results.ts",
    "hooks/reports.ts",
    "hooks/issues.ts",
  ]) {
    const text = readFileSync(join(SRC, file), "utf8");
    for (const m of text.matchAll(/export (?:async )?function (\w+)/g)) {
      // The command string appears inside the function body; take the slice
      // up to the next export as its scope.
      const start = m.index ?? 0;
      const nextExport = text.indexOf("\nexport ", start + 1);
      const body = text.slice(
        start,
        nextExport === -1 ? undefined : nextExport,
      );
      if (RUN_DERIVED_COMMANDS.some((c) => body.includes(`"${c}"`))) {
        found.add(m[1]);
      }
    }
  }
  return found;
}

/** Every `useEffect(() => { ... }, [deps])` in `text`, as (body, deps). */
function effects(text: string): { body: string; deps: string }[] {
  const out: { body: string; deps: string }[] = [];
  const OPEN = "useEffect(() => {";
  let i = text.indexOf(OPEN);
  while (i !== -1) {
    let depth = 1;
    let j = i + OPEN.length;
    while (j < text.length && depth > 0) {
      if (text[j] === "{") depth++;
      else if (text[j] === "}") depth--;
      j++;
    }
    const close = text.indexOf("]", j);
    out.push({
      body: text.slice(i + OPEN.length, j - 1),
      deps: close === -1 ? "" : text.slice(j, close + 1),
    });
    i = text.indexOf(OPEN, j);
  }
  return out;
}

describe("run-derived data stays fresh", () => {
  const wrappers = runDerivedWrappers();

  it("discovers the wrappers it is meant to guard", () => {
    // A rename that empties this set would make every assertion below vacuous.
    expect(wrappers.size).toBeGreaterThanOrEqual(4);
    expect(wrappers).toContain("probeReportBlocks");
    expect(wrappers).toContain("getPumpEnergy");
  });

  it("every effect reading a completed run re-runs when one lands", () => {
    const offenders: string[] = [];
    for (const file of walk(SRC)) {
      const text = readFileSync(file, "utf8");
      for (const { body, deps } of effects(text)) {
        const used = [...wrappers].filter((w) =>
          new RegExp(`\\b${w}\\s*\\(`).test(body),
        );
        if (used.length === 0) continue;
        if (FRESHNESS_TOKENS.some((t) => deps.includes(t))) continue;
        if (FRESHNESS_PRODUCERS.some((p) => body.includes(p))) continue;
        offenders.push(
          `${file.replace(SRC, "")} — effect calls ${used.join(", ")} ` +
            `but its dependencies ${deps.replace(/\s+/g, " ").trim()} ` +
            `contain no freshness token (${FRESHNESS_TOKENS.join(" or ")})`,
        );
      }
    }
    expect(offenders).toEqual([]);
  });
});
