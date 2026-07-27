/**
 * GUI release notes for the home page "What's new" section + modal.
 *
 * Fetches the repo's GitHub releases (paged), keeps ONLY the GUI track
 * (`gui-v*` tags — the repo also ships library `v*` and CLI `cli-v*`
 * releases), sorts semver-descending, and exposes the raw markdown bodies —
 * rendering is the caller's concern. The last successful payload is cached
 * in localStorage so the section still renders offline; every mount
 * re-fetches in the background and refreshes the cache.
 *
 * A per-machine `lastSeenGuiVersion` marker drives the "New" badge: unseen =
 * releases with version strictly greater than the marker. First run seeds
 * the marker to the running app version, so new installs see only the
 * current release with no backlog.
 */

import { useCallback, useEffect, useState } from "react";
import { getVersions } from "./projects";

const RELEASES_URL = "https://api.github.com/repos/neeraip/hydra/releases";
export const ALL_RELEASES_URL = "https://github.com/neeraip/hydra/releases";
const RELEASES_PAGE_SIZE = 5;
const RELEASES_MAX_PAGES = 4;

const CACHE_KEY = "hydra2-gui-releases-cache";
const LAST_SEEN_KEY = "hydra2-last-seen-gui-version";

type GitHubRelease = {
  tag_name: string;
  draft: boolean;
  prerelease: boolean;
  published_at?: string;
  body?: string;
  html_url?: string;
};

/** One published GUI-track release. */
export interface GuiRelease {
  /** Bare semver, `gui-v` prefix stripped. */
  version: string;
  /** Human-formatted publish date; empty when GitHub omitted it. */
  date: string;
  /** Raw release markdown (opaque — no extraction happens here). */
  body: string;
  releaseUrl: string;
}

export type ReleaseNotes =
  | { status: "loading" }
  | { status: "unavailable" }
  | { status: "loaded"; releases: GuiRelease[] };

// ── Pure helpers (unit-tested) ───────────────────────────────────────────────

/** Version of a GUI-track tag (`gui-v2.1.0` → `2.1.0`); null for any other
 * track (`v*` library, `cli-v*` CLI) or malformed tag. */
export function guiVersionFromTag(tag: string): string | null {
  if (!tag.startsWith("gui-v")) return null;
  const version = tag.slice("gui-v".length);
  return version ? version : null;
}

/** Numeric dot-segment semver comparison (−1 / 0 / +1). Missing segments
 * count as 0 (`2.1` == `2.1.0`); non-numeric suffixes are ignored, so a
 * prerelease compares equal to its release — the GUI track publishes plain
 * `x.y.z` tags. */
export function compareSemver(a: string, b: string): number {
  const pa = a.split(".");
  const pb = b.split(".");
  const len = Math.max(pa.length, pb.length);
  for (let i = 0; i < len; i += 1) {
    const na = Number.parseInt(pa[i] ?? "0", 10) || 0;
    const nb = Number.parseInt(pb[i] ?? "0", 10) || 0;
    if (na !== nb) return na < nb ? -1 : 1;
  }
  return 0;
}

/** Strip HTML comments (GitHub prepends a generated release-notes preamble
 * like `<!-- Release notes generated using … -->`) and trim. Applied at the
 * data boundary so emptiness checks ("body is only a comment") and both
 * render sites see clean markdown. */
export function stripHtmlComments(markdown: string): string {
  return markdown.replace(/<!--[\s\S]*?-->/g, "").trim();
}

/** Strip every line that is ENTIRELY a "Full Changelog: <url>" plumbing
 * line (GitHub's generated compare link — bold markers and colon optional,
 * case-insensitive), wherever it appears; some releases carry it BEFORE the
 * notes. Whole-line anchoring keeps prose mentions safe ("see the full
 * changelog for details" survives). CRLF endings are normalized first so
 * stray `\r` can never defeat the match; a formatting miss simply leaves
 * the line visible. */
export function stripChangelogLines(markdown: string): string {
  return markdown
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .filter(
      (line) => !/^\s*\*{0,2}full changelog\*{0,2}:?\s*\S+\s*$/i.test(line),
    )
    .join("\n")
    .trim();
}

/** Thematic-break line (`---` / `***` / `___`, CommonMark's ≤3 leading
 * spaces). */
const THEMATIC_BREAK = /^\s{0,3}(?:-{3,}|\*{3,}|_{3,})\s*$/;
/** The unsigned-installer boilerplate heading (any level, trimmed line). */
const INSTALL_NOTE_HEADING = /^#{1,6}\s*installation note\s*$/i;

/** Strip the repo's TRAILING "Installation Note" boilerplate: the section
 * starting at the LAST thematic-break line whose first non-blank following
 * line is an `## Installation Note` heading (any level), through the end.
 * Conservative: if the last break is not immediately followed by that
 * heading — or the heading appears without a preceding break — the body is
 * left untouched. CRLF endings are normalized first. */
export function stripTrailingInstallationNote(markdown: string): string {
  const normalized = markdown.replace(/\r\n?/g, "\n");
  const lines = normalized.split("\n");
  for (let i = lines.length - 1; i >= 0; i -= 1) {
    if (!THEMATIC_BREAK.test(lines[i])) continue;
    // Found the last thematic break; the boilerplate heading must be the
    // first non-blank line after it.
    let j = i + 1;
    while (j < lines.length && lines[j].trim() === "") j += 1;
    if (j < lines.length && INSTALL_NOTE_HEADING.test(lines[j].trim())) {
      return lines.slice(0, i).join("\n").trim();
    }
    return normalized.trim();
  }
  return normalized.trim();
}

/** Data-boundary release-body cleanup: HTML comments out, then changelog
 * plumbing lines, then the trailing installation-note boilerplate. Both
 * render sites and the emptiness checks ("body is only plumbing") see the
 * cleaned markdown. */
export function cleanReleaseBody(markdown: string): string {
  return stripTrailingInstallationNote(
    stripChangelogLines(stripHtmlComments(markdown)),
  );
}

/** Whether a release has any notes left after cleanup. Both surfaces use
 * this to decide between rendering the body and the explicit
 * "No release notes" empty state. */
export function releaseHasNotes(release: GuiRelease): boolean {
  return release.body.trim() !== "";
}

/** Releases with actual notes after cleanup. Drives the "+N earlier
 * updates" teaser count — the number should promise reading material, so
 * plumbing-only (empty-body) releases are excluded. (They still appear in
 * the modal stack as compact "No release notes" rows and count as seen when
 * the marker advances.) */
export function releasesWithContent(releases: GuiRelease[]): GuiRelease[] {
  return releases.filter(releaseHasNotes);
}

/** Newest-first copy of `releases`. */
export function sortReleasesDesc(releases: GuiRelease[]): GuiRelease[] {
  return [...releases].sort((a, b) => compareSemver(b.version, a.version));
}

/** Releases strictly newer than the last-seen marker. A `null` marker means
 * "not seeded yet" — nothing counts as unseen (no backlog flash while the
 * app version loads on first run). */
export function unseenReleases(
  releases: GuiRelease[],
  lastSeen: string | null,
): GuiRelease[] {
  if (lastSeen === null) return [];
  return releases.filter((r) => compareSemver(r.version, lastSeen) > 0);
}

/** Versions whose accordion items start expanded in the release-notes
 * modal: every unseen (strictly newer than the marker) release, plus the
 * newest release even when already seen. Notes-less releases render as
 * compact rows with nothing to expand, so they are never included. */
export function defaultExpandedVersions(
  releases: GuiRelease[],
  lastSeen: string | null,
): Set<string> {
  const newest = releases[0] ?? null;
  return new Set(
    releases
      .filter(
        (r) =>
          releaseHasNotes(r) &&
          (r === newest ||
            (lastSeen !== null && compareSemver(r.version, lastSeen) > 0)),
      )
      .map((r) => r.version),
  );
}

// ── Fetch + cache ────────────────────────────────────────────────────────────

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-GB", {
    day: "numeric",
    month: "long",
    year: "numeric",
  });
}

function readCachedReleases(): GuiRelease[] | null {
  try {
    if (typeof localStorage === "undefined") return null;
    const raw = localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed) || parsed.length === 0) return null;
    const rows = parsed
      .filter(
        (r): r is GuiRelease =>
          typeof r === "object" &&
          r !== null &&
          typeof (r as GuiRelease).version === "string" &&
          typeof (r as GuiRelease).body === "string",
      )
      // Re-clean on read so caches written before body cleanup existed
      // stay clean.
      .map((r) => ({ ...r, body: cleanReleaseBody(r.body) }));
    return rows.length > 0 ? rows : null;
  } catch {
    return null;
  }
}

function writeCachedReleases(releases: GuiRelease[]): void {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(CACHE_KEY, JSON.stringify(releases));
    }
  } catch {
    // Cache persistence is best-effort.
  }
}

/** Page through the releases API collecting every published GUI release.
 * Returns null on network/API failure. */
async function fetchGuiReleases(): Promise<GuiRelease[] | null> {
  try {
    const collected: GuiRelease[] = [];
    for (let page = 1; page <= RELEASES_MAX_PAGES; page += 1) {
      const params = new URLSearchParams({
        per_page: String(RELEASES_PAGE_SIZE),
        page: String(page),
      });
      const res = await fetch(`${RELEASES_URL}?${params.toString()}`, {
        headers: { Accept: "application/vnd.github+json" },
      });
      if (!res.ok) return null;
      const releases: GitHubRelease[] = await res.json();
      for (const r of releases) {
        const version = guiVersionFromTag(r.tag_name);
        if (!version || r.draft || r.prerelease) continue;
        collected.push({
          version,
          date: r.published_at ? formatDate(r.published_at) : "",
          body: cleanReleaseBody(r.body ?? ""),
          releaseUrl: r.html_url ?? "",
        });
      }
      if (releases.length < RELEASES_PAGE_SIZE) break;
    }
    return sortReleasesDesc(collected);
  } catch {
    return null;
  }
}

/** GUI release list: cache-first (renders offline), refreshed in the
 * background on every mount. */
export function useReleaseNotes(): ReleaseNotes {
  const [notes, setNotes] = useState<ReleaseNotes>(() => {
    const cached = readCachedReleases();
    return cached
      ? { status: "loaded", releases: cached }
      : { status: "loading" };
  });

  useEffect(() => {
    let cancelled = false;
    void fetchGuiReleases().then((list) => {
      if (cancelled) return;
      if (list && list.length > 0) {
        writeCachedReleases(list);
        setNotes({ status: "loaded", releases: list });
      } else {
        // Keep serving the cache on failure; only report unavailable when
        // there is nothing at all to show.
        setNotes((prev) =>
          prev.status === "loaded" ? prev : { status: "unavailable" },
        );
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  return notes;
}

// ── Last-seen marker ─────────────────────────────────────────────────────────

function readLastSeen(): string | null {
  try {
    if (typeof localStorage === "undefined") return null;
    return localStorage.getItem(LAST_SEEN_KEY);
  } catch {
    return null;
  }
}

function writeLastSeen(version: string): void {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(LAST_SEEN_KEY, version);
    }
  } catch {
    // Persistence is best-effort.
  }
}

/**
 * Per-machine last-seen GUI version. `lastSeen` is null until first-run
 * seeding completes (during which nothing counts as unseen). `markSeen`
 * persists a new marker — the What's-new modal calls it on close with the
 * newest fetched version.
 */
export function useLastSeenGuiVersion(): {
  lastSeen: string | null;
  markSeen: (version: string) => void;
} {
  const [lastSeen, setLastSeen] = useState<string | null>(() => readLastSeen());

  // First-run seeding: no stored marker → the running app version, so a
  // fresh install sees only the current release, never the backlog. The
  // "0.0.0" fallback (outside a Tauri shell) is NOT persisted — it would
  // mark the entire history unseen.
  useEffect(() => {
    if (lastSeen !== null) return;
    let cancelled = false;
    getVersions()
      .then((v) => {
        if (cancelled || !v.app || v.app === "0.0.0") return;
        writeLastSeen(v.app);
        setLastSeen(v.app);
      })
      .catch(() => {
        // Leave unseeded — unseen stays empty.
      });
    return () => {
      cancelled = true;
    };
  }, [lastSeen]);

  const markSeen = useCallback((version: string) => {
    writeLastSeen(version);
    setLastSeen(version);
  }, []);

  return { lastSeen, markSeen };
}
