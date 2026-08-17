/**
 * The archive-import review's decisions, as plain data in and plain data
 * out: which entries start included, what each project is called, which
 * rows may be created, and what the create call sends.
 *
 * Extracted from the wizard so each decision has a name and a test — a
 * ternary inside the table's JSX is a decision nothing can call.
 */

import type { ArchiveModelEntry, ArchiveScan, SidecarRef } from "../../hooks";

/** One review-table row: an archive entry plus the user's answers. */
export interface ArchiveRow {
  /** Entry path inside the archive — the row's identity. */
  path: string;
  /** Project name, editable; seeded from the file stem. */
  name: string;
  /** The engine that will parse it: recognised, or chosen for an
   * ambiguous entry. `null` until the ambiguity is answered. */
  engine: string | null;
  /** GUI-openable possibilities when recognition was ambiguous. */
  candidates: string[];
  include: boolean;
  nodeCount: number;
  linkCount: number;
  findingCount: number;
  repairs: string[];
  sidecars: SidecarRef[];
  error: string | null;
}

/**
 * Whether a row can become a project at all: it needs an engine and no
 * scan error. Rows that cannot are shown — the user must see what will
 * not import and why — but never selectable.
 */
export function rowImportable(row: {
  engine: string | null;
  error: string | null;
}): boolean {
  return row.engine !== null && row.error === null;
}

/**
 * Seed the review table from a scan: importable entries start included,
 * ambiguous ones excluded until their engine is chosen, failed ones
 * permanently excluded. Names seed from the file stem — the one piece of
 * intent an archive carries.
 */
export function rowsFromScan(scan: ArchiveScan): ArchiveRow[] {
  return scan.models.map((entry: ArchiveModelEntry) => ({
    path: entry.path,
    name: entry.stem.trim() || "Untitled Project",
    engine: entry.engine,
    candidates: entry.candidates,
    include: rowImportable(entry),
    nodeCount: entry.nodeCount,
    linkCount: entry.linkCount,
    findingCount: entry.findingCount,
    repairs: entry.repairs,
    sidecars: entry.sidecars,
    error: entry.error,
  }));
}

/**
 * Answer an ambiguous row's engine. Choosing also includes the row —
 * naming an engine for a model you are not importing is not a thing the
 * table offers — while any other row passes through untouched.
 */
export function withEngineChosen(
  rows: ArchiveRow[],
  path: string,
  engine: string,
): ArchiveRow[] {
  return rows.map((row) =>
    row.path === path && row.error === null
      ? { ...row, engine, include: true }
      : row,
  );
}

/**
 * What the create call sends: every included row that can import, with
 * blank names falling back rather than creating a nameless project.
 */
export function selectionsFrom(
  rows: ArchiveRow[],
): { path: string; name: string; engine: string }[] {
  return rows
    .filter((row) => row.include && rowImportable(row))
    .map((row) => ({
      path: row.path,
      name: row.name.trim() || "Untitled Project",
      // rowImportable above guarantees engine is present.
      engine: row.engine as string,
    }));
}

/**
 * What a row's sidecar references amount to: the carried ones travel with
 * the project silently well, the missing ones are the warning. Named so
 * the wizard's icon and tooltip cannot drift from each other.
 */
export function sidecarNote(
  sidecars: SidecarRef[],
): { tone: "ok" | "warn"; text: string } | null {
  if (sidecars.length === 0) return null;
  // A reference the run cannot consume outranks everything: whether or
  // not the archive holds the file, the model needs a capability, not a
  // byte stream, and green here would promise one.
  const unsupported = sidecars.filter((s) => !s.supported);
  if (unsupported.length > 0) {
    return {
      tone: "warn",
      text: `Uses ${unsupported.map((s) => s.label).join(", ")}. Not supported yet: runs will refuse or read the data as empty`,
    };
  }
  const missing = sidecars.filter((s) => !s.carried);
  if (missing.length === 0) {
    return {
      tone: "ok",
      text: `Imports ${sidecars.map((s) => s.label).join(", ")} with the project`,
    };
  }
  return {
    tone: "warn",
    text: `References ${missing.map((s) => s.label).join(", ")}. Not in this archive: runs will refuse until the data is supplied`,
  };
}

/**
 * The footer's one-line account of the archive's non-model entries.
 *
 * It must not claim these are left behind: the data files a selected
 * model references *are* carried now, and the rows say so per model. So
 * the footer states what these entries are — everything that is not a
 * model — and leaves "imported or not" to the row that knows.
 */
export function leftBehindSummary(others: string[]): string {
  if (others.length === 0) return "";
  const shown = others.slice(0, 3).join(", ");
  const more = others.length > 3 ? ` and ${others.length - 3} more` : "";
  return `Other files in this archive: ${shown}${more}. Imported only where a selected model reads them`;
}
