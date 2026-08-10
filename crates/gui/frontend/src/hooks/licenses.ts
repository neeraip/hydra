/**
 * What the app can say about its own licensing.
 *
 * Three documents live behind these calls: Hydra's own licence, the
 * commercial-licence offer, and the notices of every open-source package
 * this build ships. All three are embedded in the binary — see
 * `crates/gui/src/commands/about.rs` — so they are readable on a machine
 * that has never seen the repository.
 *
 * The notices are split deliberately. The component list is small enough to
 * fetch whole; the licence texts are a megabyte between them and are
 * fetched one at a time, by the index a component names.
 */

import { tryInvoke, tryInvokeOr } from "./ipc";

/** Hydra's own licensing. */
export interface LicenseInfo {
  /** SPDX identifier — `AGPL-3.0-only`. */
  spdx: string;
  /** The full licence text. */
  text: string;
  /** The commercial-licence document, in Markdown. */
  commercial: string;
  /** Where the corresponding source is published. */
  sourceUrl: string;
}

/** One licence file a package ships. */
export interface NoticeFile {
  /** The file's name in the package (`LICENSE`, `LICENSE-APACHE`, …). */
  name: string;
  /** Index into the shared text pool, for `getThirdPartyLicenseText`. */
  text: number;
}

/** One open-source package this build ships. */
export interface ThirdPartyComponent {
  name: string;
  version: string;
  /** `"rust"` or `"npm"`. */
  ecosystem: string;
  /** The declared SPDX expression; empty when the package declares none. */
  spdx: string;
  /** The package's own home; empty when it states none. */
  url: string;
  files: NoticeFile[];
}

export async function getLicenseInfo(): Promise<LicenseInfo | null> {
  return tryInvoke<LicenseInfo>("get_license_info");
}

export async function listThirdPartyComponents(): Promise<
  ThirdPartyComponent[]
> {
  return tryInvokeOr<ThirdPartyComponent[]>(
    "list_third_party_components",
    undefined,
    [],
  );
}

export async function getThirdPartyLicenseText(
  index: number,
): Promise<string | null> {
  return tryInvoke<string>("get_third_party_license_text", { index });
}

// ── Decisions ─────────────────────────────────────────────────────────────────

/**
 * Whether a component answers to what was typed.
 *
 * Matched against the version too, which is not obvious but is the reason
 * the box is there: the question a reader brings to a notices list is
 * almost always "is *this* package in here, and which version", arriving
 * from an advisory that names both.
 */
export function matchesComponentQuery(
  component: ThirdPartyComponent,
  query: string,
): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return [
    component.name,
    component.version,
    component.spdx,
    component.ecosystem,
  ].some((field) => field.toLowerCase().includes(q));
}

/**
 * What stands in for a licence text when the package ships none.
 *
 * Around eighty do: they declare a licence in their manifest and leave it
 * at that. Their row is the whole notice they get, so it has to say what
 * the licence is and where the package lives rather than showing an empty
 * panel that reads as a missing notice.
 *
 * Returns null when the package ships its own text, which is the normal
 * case and needs no substitute.
 */
export function noticeFallback(component: ThirdPartyComponent): string | null {
  if (component.files.length > 0) return null;
  const licence = component.spdx
    ? `Declared as ${component.spdx}.`
    : "No licence declared.";
  return component.url
    ? `${licence} This package ships no licence file; see ${component.url}.`
    : `${licence} This package ships no licence file.`;
}

/**
 * A link in an embedded repository document, resolved against the source.
 *
 * The commercial-licence document is the repository's own Markdown, and it
 * points at sibling files — `[the AGPL](LICENSE)`. Read from the
 * repository that resolves; read from inside an installed app there is no
 * sibling to resolve to, and the link opens nothing at all. Rewriting
 * relative targets onto the published source is what makes the document
 * still work once it has left the tree it was written in.
 */
export function repoLink(href: string, sourceUrl: string): string {
  if (!href) return href;
  // Already somewhere, or going nowhere: absolute URLs, mail links and
  // in-document anchors are all left exactly as written.
  if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith("#")) return href;
  if (href.startsWith("//")) return href;
  const path = href.replace(/^\.\//, "").replace(/^\//, "");
  return `${sourceUrl.replace(/\/+$/, "")}/blob/main/${path}`;
}
