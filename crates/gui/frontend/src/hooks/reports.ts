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

/** Build the template JSON for the builder's current state: enabled block
 * ids in display order, carrying any per-block options. Options are held
 * opaquely so values the builder cannot render — a hand-authored key, or one
 * from a newer engine — still survive the round-trip. Matches
 * `crates/report` template format v1. */
export function buildTemplateJson(
  title: string,
  orderedIds: string[],
  enabled: ReadonlySet<string>,
  optionsById: Readonly<Record<string, unknown>> = {},
): string {
  return JSON.stringify(
    {
      version: 1,
      title,
      blocks: orderedIds
        .filter((id) => enabled.has(id))
        .map((id) =>
          id in optionsById ? { id, options: optionsById[id] } : { id },
        ),
    },
    null,
    2,
  );
}

/** Parse a saved template into builder state against the current catalog:
 * returns the title, the full id order (template order first, then any
 * catalog blocks the template omitted), and the enabled set (only ids the
 * template listed). Unknown template ids are dropped — the catalog is the
 * authority on what exists. */
export function builderStateFromTemplate(
  templateJson: string,
  catalog: ReportBlockInfo[],
): {
  title: string;
  order: string[];
  enabled: Set<string>;
  optionsById: Record<string, unknown>;
} | null {
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
      .map((b) => b as { id?: unknown; options?: unknown })
      .filter((b): b is { id: string; options?: unknown } => {
        return typeof b.id === "string";
      });
    const known = new Set(catalog.map((b) => b.id));
    const order: string[] = [];
    const optionsById: Record<string, unknown> = {};
    for (const block of listed) {
      if (!known.has(block.id) || order.includes(block.id)) continue;
      order.push(block.id);
      if (block.options !== undefined) optionsById[block.id] = block.options;
    }
    const enabled = new Set(order);
    for (const block of catalog) {
      if (!order.includes(block.id)) {
        order.push(block.id);
      }
    }
    return {
      title: (parsed as { title: string }).title,
      order,
      enabled,
      optionsById,
    };
  } catch {
    return null;
  }
}
