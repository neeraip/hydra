/**
 * Report-view commands: block catalog, per-project template persistence,
 * document generation (preview + export). The template JSON produced here
 * is the same format `hydra report --template` consumes headlessly.
 */

import { invoke, tryInvoke, tryInvokeOr } from "./ipc";

/** One entry of the engine's report-block catalog. */
export interface ReportBlockInfo {
  id: string;
  title: string;
  summary: string;
}

export type ReportFormat = "txt" | "csv" | "html" | "pdf";

/** One selectable item of a `choice` / `multiChoice` option. */
export interface ChoiceItem {
  value: string;
  label: string;
}

/** Value shape of a describable block option (hydra-common spec §3.2.1).
 * Discriminated by `type`; bounds and defaults are advisory — the engine
 * validates independently at production time. */
export type OptionKind =
  | {
      type: "number";
      default: number | null;
      min: number | null;
      max: number | null;
    }
  | {
      type: "integer";
      default: number | null;
      min: number | null;
      max: number | null;
    }
  | { type: "boolean"; default: boolean | null }
  | { type: "text"; default: string | null }
  | {
      type: "numberList";
      default: number[] | null;
      minLen: number | null;
      ascending: boolean;
    }
  | { type: "choice"; default: string | null; items: ChoiceItem[] }
  | { type: "multiChoice"; default: string[] | null; items: ChoiceItem[] };

/** One option a block accepts, resolved against the target's model — so
 * defaults and units already match that model's unit system. */
export interface ReportOptionInfo {
  key: string;
  label: string;
  help: string;
  kind: OptionKind;
  unit: string | null;
}

/** Whether a block can be produced for the current target. */
export interface BlockAvailability {
  id: string;
  status: "ok" | "unavailable" | "failed";
  reason?: string;
}

/** Which catalog blocks apply to this target's run. Empty when the target has
 * no results yet — nothing can be produced, so nothing is flagged. */
export async function probeReportBlocks(
  projectId: string,
  scenarioId: string | null,
): Promise<BlockAvailability[]> {
  return tryInvokeOr<BlockAvailability[]>(
    "probe_report_blocks",
    { projectId, scenarioId },
    [],
  );
}

/** The options `blockId` accepts for this target. Empty when the block takes
 * none, or when the backend cannot describe it. */
export async function getReportBlockOptions(
  projectId: string,
  scenarioId: string | null,
  blockId: string,
): Promise<ReportOptionInfo[]> {
  return tryInvokeOr<ReportOptionInfo[]>(
    "get_report_block_options",
    { projectId, scenarioId, blockId },
    [],
  );
}

/** The report-block catalog of the project's engine. */
export async function listReportBlocks(): Promise<ReportBlockInfo[]> {
  return tryInvokeOr<ReportBlockInfo[]>("list_report_blocks", undefined, []);
}

/** The project's saved template JSON, or null before one exists. */
export async function getReportTemplate(
  projectId: string,
): Promise<string | null> {
  return tryInvoke<string | null>("get_report_template", { projectId });
}

/** Persist the project's template JSON (validated backend-side). */
export async function saveReportTemplate(
  projectId: string,
  templateJson: string,
): Promise<void> {
  await tryInvoke("save_report_template", { projectId, templateJson });
}

/** Render a report document (`pdf` resolves to base64-encoded bytes; the
 * other formats resolve to the text verbatim). Throws with a user-facing
 * message when the target has no results or the template is invalid. */
export async function generateReport(args: {
  projectId: string;
  scenarioId: string | null;
  templateJson: string;
  format: ReportFormat;
  withTimestamp: boolean;
}): Promise<string> {
  return invoke<string>("generate_report", {
    projectId: args.projectId,
    scenarioId: args.scenarioId,
    templateJson: args.templateJson,
    format: args.format,
    withTimestamp: args.withTimestamp,
  });
}

/** Generate (with timestamp) and save via the OS dialog. Resolves to the
 * chosen path, or null when the user cancelled. Throws on failure. */
export async function exportReport(args: {
  projectId: string;
  scenarioId: string | null;
  templateJson: string;
  format: ReportFormat;
}): Promise<string | null> {
  return invoke<string | null>("export_report", {
    projectId: args.projectId,
    scenarioId: args.scenarioId,
    templateJson: args.templateJson,
    format: args.format,
  });
}

/** The builder's state: exactly the sections in the report, in order, with
 * any per-section heading override and options. A block that is not listed
 * is simply not in the report — there is no separate enabled flag, because
 * the template format has no such concept either. */
export interface BuilderState {
  title: string;
  /** Block ids in document order. */
  sections: string[];
  /** Per-block heading override, replacing the block's default heading. */
  headingById: Record<string, string>;
  /** Per-block options, opaque. */
  optionsById: Record<string, unknown>;
}

/** Serialise builder state to template JSON (`crates/report` format v1) —
 * the same file `hydra report --template` consumes. Options and headings are
 * written only when set, so a report left at its defaults produces the same
 * bytes a hand-authored default template would. */
export function buildTemplateJson(state: BuilderState): string {
  return JSON.stringify(
    {
      version: 1,
      title: state.title,
      blocks: state.sections.map((id) => {
        const heading = state.headingById[id]?.trim();
        return {
          id,
          ...(heading ? { title: heading } : {}),
          ...(id in state.optionsById
            ? { options: state.optionsById[id] }
            : {}),
        };
      }),
    },
    null,
    2,
  );
}

/** Restore builder state from saved template JSON, or null when the file is
 * not a template this build reads.
 *
 * Ids the catalog does not know are dropped: a template may outlive the block
 * it names, and keeping it would put a row in the outline that can never
 * render. Duplicates collapse to their first occurrence. */
export function builderStateFromTemplate(
  templateJson: string,
  catalog: ReportBlockInfo[],
): BuilderState | null {
  try {
    const parsed: unknown = JSON.parse(templateJson);
    if (
      typeof parsed !== "object" ||
      parsed === null ||
      (parsed as { version?: unknown }).version !== 1 ||
      typeof (parsed as { title?: unknown }).title !== "string"
    ) {
      return null;
    }
    const raw = (parsed as { blocks?: unknown }).blocks;
    const listed = (Array.isArray(raw) ? raw : [])
      .map((b) => b as { id?: unknown; title?: unknown; options?: unknown })
      .filter((b): b is { id: string; title?: unknown; options?: unknown } => {
        return typeof b.id === "string";
      });
    const known = new Set(catalog.map((b) => b.id));
    const sections: string[] = [];
    const headingById: Record<string, string> = {};
    const optionsById: Record<string, unknown> = {};
    for (const block of listed) {
      if (!known.has(block.id) || sections.includes(block.id)) continue;
      sections.push(block.id);
      if (typeof block.title === "string") headingById[block.id] = block.title;
      if (block.options !== undefined) optionsById[block.id] = block.options;
    }
    return {
      title: (parsed as { title: string }).title,
      sections,
      headingById,
      optionsById,
    };
  } catch {
    return null;
  }
}

/** Move `from` to `to`, returning a new array. Out-of-range indices leave the
 * list untouched, so a drop outside the list is a no-op rather than a
 * reordering nobody asked for. */
export function moveSection(
  sections: readonly string[],
  from: number,
  to: number,
): string[] {
  if (
    from === to ||
    from < 0 ||
    to < 0 ||
    from >= sections.length ||
    to >= sections.length
  ) {
    return [...sections];
  }
  const next = [...sections];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved);
  return next;
}

// ── Remembered preview format ────────────────────────────────────────────────

/** Minimal storage surface, so the helpers below are testable without a DOM
 * and degrade to a no-op where storage is unavailable or blocked. */
export interface FormatStore {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

const FORMAT_KEY_PREFIX = "hydra2-report-format:";

const FORMATS: readonly ReportFormat[] = ["txt", "csv", "html", "pdf"];

/** Keyed by PROJECT, not scenario: the format is a property of the report you
 * are producing, and switching between a project's scenarios to compare them
 * should not change what you are looking at. */
function formatKey(projectId: string): string {
  return `${FORMAT_KEY_PREFIX}${projectId}`;
}

/** The format last chosen for this project, or `fallback` when there is none
 * — or when the stored value is not one this build offers, which is how a
 * format retired between releases stops resolving. */
export function readStoredFormat(
  projectId: string,
  fallback: ReportFormat,
  store: FormatStore | undefined = safeStorage(),
): ReportFormat {
  const raw = store?.getItem(formatKey(projectId)) ?? null;
  return FORMATS.find((f) => f === raw) ?? fallback;
}

/** Remember `format` for this project. Failure is ignored: a preference not
 * persisting is not worth interrupting a report for. */
export function writeStoredFormat(
  projectId: string,
  format: ReportFormat,
  store: FormatStore | undefined = safeStorage(),
): void {
  try {
    store?.setItem(formatKey(projectId), format);
  } catch {
    // Private browsing, a full quota, or storage disabled entirely.
  }
}

function safeStorage(): FormatStore | undefined {
  try {
    return globalThis.localStorage ?? undefined;
  } catch {
    return undefined;
  }
}

/** How far row `index` steps aside while row `from` is being dragged to
 * `dest`, in pixels.
 *
 * Rows the dragged row passes on its way down move up by one slot; rows it
 * passes on the way up move down by one. Everything outside that span, and
 * the dragged row itself, stays put. `slot` is the space the dragged row
 * frees — its own height plus the gap — which is the displacement regardless
 * of how tall the rows being passed happen to be. */
export function rowShift(
  index: number,
  from: number,
  dest: number,
  slot: number,
): number {
  if (index === from) return 0;
  if (from < index && index <= dest) return -slot;
  if (dest <= index && index < from) return slot;
  return 0;
}

/** The report's sections rearranged into catalog order.
 *
 * Membership is untouched — this fixes the ORDER of what is already in the
 * report, and does not add back what was deliberately removed. The catalog's
 * own order is the recommendation: it runs summary → results → diagnostics,
 * which is the order the sections read in.
 *
 * Ids the catalog does not rank sort to the end, keeping their relative
 * order, so an unknown section is never silently dropped. */
export function recommendedOrder(
  catalog: readonly ReportBlockInfo[],
  sections: readonly string[],
): string[] {
  const rank = new Map(catalog.map((block, i) => [block.id, i]));
  return [...sections].sort(
    (a, b) =>
      (rank.get(a) ?? Number.POSITIVE_INFINITY) -
      (rank.get(b) ?? Number.POSITIVE_INFINITY),
  );
}

/** Whether two section lists hold the same ids in the same order. */
export function sameOrder(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((id, i) => id === b[i]);
}

/** Human labels for everything customised on a section, for showing WHY it is
 * marked as changed. Empty when the section is entirely at its defaults.
 *
 * Falls back to the raw key when no descriptor matches: an option authored by
 * hand, or one from an engine newer than this build, is still a customisation
 * and saying so beats pretending the section is untouched. */
export function customisedSummary(
  descriptors: readonly ReportOptionInfo[],
  values: Readonly<Record<string, unknown>> | undefined,
  heading: string,
): string[] {
  const labels: string[] = [];
  if (heading.trim() !== "") labels.push("Heading");
  for (const key of Object.keys(values ?? {})) {
    const descriptor = descriptors.find((d) => d.key === key);
    labels.push(descriptor?.label ?? key);
  }
  return labels;
}

/** Convert a drop slot into the destination index [`moveSection`] expects.
 *
 * A drop targets a GAP: slot `i` means "before row i", and slot `n` means
 * "at the end". Once the dragged row is lifted out, every slot after it
 * shifts down by one — so dropping row 0 into slot 2 lands at index 1, not
 * 2. Getting this wrong makes a downward drag land one row short. */
export function insertionToIndex(from: number, insertion: number): number {
  return insertion > from ? insertion - 1 : insertion;
}

/** The drop slot for a pointer at `y`, given each row's bounds in order.
 *
 * Rows are split at their midpoint: above it the pointer targets the gap
 * before the row, below it the gap after. Returns `rows.length` when the
 * pointer is past the last midpoint. */
export function insertionFromPointer(
  rows: readonly { top: number; height: number }[],
  y: number,
): number {
  for (let i = 0; i < rows.length; i++) {
    if (y < rows[i].top + rows[i].height / 2) return i;
  }
  return rows.length;
}

/** Catalog entries not yet in the report, in catalog order, narrowed by a
 * case-insensitive match on title or summary. */
export function addableBlocks(
  catalog: readonly ReportBlockInfo[],
  sections: readonly string[],
  query: string,
): ReportBlockInfo[] {
  const inReport = new Set(sections);
  const needle = query.trim().toLowerCase();
  return catalog.filter((block) => {
    if (inReport.has(block.id)) return false;
    if (needle === "") return true;
    return (
      block.title.toLowerCase().includes(needle) ||
      block.summary.toLowerCase().includes(needle)
    );
  });
}
