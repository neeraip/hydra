#!/usr/bin/env node
/**
 * Build-time CRS catalog generator.
 *
 * Reads all geographic and projected CRS definitions from @esri/proj-codes
 * (covers both EPSG and Esri-authority codes), probes each entry against
 * proj4js to verify it parses and can form a WGS84 conversion, then writes
 * a frozen JSON snapshot to the frontend public directory.
 *
 * The snapshot is committed to the repo and loaded by the app at runtime as
 * a static asset — no network access required.
 *
 * Usage:
 *   node scripts/update-crs-catalog.mjs
 *   # or via just:
 *   just update-crs-catalog
 */

import { createRequire } from "module";
import { createWriteStream } from "fs";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";
import zlib from "zlib";

const require = createRequire(import.meta.url);
const __dir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dir, "..");
const outPath = resolve(
  repoRoot,
  "crates/gui/frontend/public/crs-catalog.json",
);

// ── Load source data ──────────────────────────────────────────────────────────

const frontendModules = resolve(
  repoRoot,
  "crates/gui/frontend/node_modules",
);

const proj4 = require(resolve(frontendModules, "proj4/dist/proj4.js"));

const geogcs = require(
  resolve(frontendModules, "@esri/proj-codes/pe_list_geogcs.json"),
).GeographicCoordinateSystems;
const projcs = require(
  resolve(frontendModules, "@esri/proj-codes/pe_list_projcs.json"),
).ProjectedCoordinateSystems;

const all = [...geogcs, ...projcs];

// ── Build catalog ─────────────────────────────────────────────────────────────

const catalog = {};
let skippedDeprecated = 0;
let skippedUnparseable = 0;

for (const entry of all) {
  if (entry.deprecated === "yes") {
    skippedDeprecated++;
    continue;
  }

  // Use latestWkid where available so lookups by current code work.
  const wkid = entry.latestWkid ?? entry.wkid;
  const code = `${entry.authority}:${wkid}`;
  const wkt = entry.wkt;

  if (!wkt) {
    skippedUnparseable++;
    continue;
  }

  try {
    proj4.defs(code, wkt);
    proj4(code, "EPSG:4326"); // force the parser; throws for unsupported projections
    catalog[code] = wkt;
  } catch {
    skippedUnparseable++;
  }
}

// ── Write output ──────────────────────────────────────────────────────────────

const json = JSON.stringify(catalog);
const buf = Buffer.from(json);
const compressed = zlib.gzipSync(buf);

const stream = createWriteStream(outPath);
stream.write(json);
stream.end();

stream.on("finish", () => {
  const entries = Object.keys(catalog).length;
  const epsg = Object.keys(catalog).filter((k) => k.startsWith("EPSG:")).length;
  const esri = Object.keys(catalog).filter((k) => k.startsWith("Esri:")).length;

  console.log(`CRS catalog written to ${outPath}`);
  console.log(`  entries:            ${entries}`);
  console.log(`    EPSG:             ${epsg}`);
  console.log(`    Esri:             ${esri}`);
  console.log(`  skipped deprecated: ${skippedDeprecated}`);
  console.log(`  skipped unparseable:${skippedUnparseable}`);
  console.log(`  raw size:           ${(buf.length / 1024).toFixed(1)} KB`);
  console.log(`  gzip size:          ${(compressed.length / 1024).toFixed(1)} KB`);
});

stream.on("error", (err) => {
  console.error("Failed to write catalog:", err.message);
  process.exit(1);
});
