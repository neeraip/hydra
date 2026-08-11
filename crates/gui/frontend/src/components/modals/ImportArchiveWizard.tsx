/**
 * ImportArchiveWizard — review and commit a multi-model archive import.
 *
 * The backend has already scanned the chosen `.zip`: recognised each
 * model-shaped entry, trial-parsed it exactly as a single-file import
 * would, and listed everything else. This modal is the review — one row
 * per model, includable, nameable, engine answered where recognition
 * could not — and then the commit, one ordinary project per included row.
 *
 * Deliberately not a step of `NewProjectWizard`: that flow is one model
 * becoming one project with the model held in managed state; this one is
 * N models becoming N projects with nothing in managed state at all.
 * The two share the backend's parse, which is what keeps them honest.
 *
 * Per-entry failure is data here, not an exception: outcomes render
 * beside the rows they belong to, successes and failures side by side.
 */

import { CheckIcon, ExclamationTriangleIcon } from "@heroicons/react/16/solid";
import { useMemo, useState } from "react";
import {
  type ArchiveImportOutcome,
  type ArchiveScan,
  createProjectsFromArchive,
  engineByKey,
  useEngines,
} from "../../hooks";
import { formatIpcError } from "../../hooks/ipc";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";
import { PrimaryButton } from "../ui/PrimaryButton";
import {
  type ArchiveRow,
  leftBehindSummary,
  rowImportable,
  rowsFromScan,
  selectionsFrom,
  withEngineChosen,
} from "./archiveImport";

const CELL: React.CSSProperties = {
  padding: "7px 10px",
  fontSize: "var(--text-md)",
  color: "var(--text-primary)",
  borderBottom: "1px solid var(--border)",
  textAlign: "left",
  verticalAlign: "middle",
};

const HEAD_CELL: React.CSSProperties = {
  ...CELL,
  fontSize: "var(--text-sm)",
  fontWeight: 600,
  color: "var(--text-secondary)",
  whiteSpace: "nowrap",
};

export function ImportArchiveWizard({
  scan,
  onClose,
  onDone,
}: {
  scan: ArchiveScan;
  onClose: () => void;
  /** Called after a create in which at least one project landed. */
  onDone: (createdCount: number) => void;
}) {
  const engines = useEngines();
  const [rows, setRows] = useState<ArchiveRow[]>(() => rowsFromScan(scan));
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  // Per-path outcomes once a create has run; the table becomes the report.
  const [outcomes, setOutcomes] = useState<Map<
    string,
    ArchiveImportOutcome
  > | null>(null);

  const selections = useMemo(() => selectionsFrom(rows), [rows]);
  const createdCount = useMemo(
    () =>
      outcomes
        ? [...outcomes.values()].filter((o) => o.project !== null).length
        : 0,
    [outcomes],
  );

  const engineLabel = (key: string) => engineByKey(engines, key)?.label ?? key;

  function patchRow(path: string, patch: Partial<ArchiveRow>) {
    setRows((prev) =>
      prev.map((row) => (row.path === path ? { ...row, ...patch } : row)),
    );
  }

  async function handleCreate() {
    if (busy || selections.length === 0) return;
    setBusy(true);
    setFailure(null);
    try {
      const results = await createProjectsFromArchive(
        scan.archivePath,
        selections,
      );
      setOutcomes(new Map(results.map((o) => [o.path, o])));
    } catch (e) {
      // The archive itself became unreadable between scan and create
      // (moved, deleted). Per-entry failures never land here.
      setFailure(formatIpcError(e));
    } finally {
      setBusy(false);
    }
  }

  /** The row's trailing word: what happened, or what stands in the way. */
  function noteFor(row: ArchiveRow): React.ReactNode {
    const outcome = outcomes?.get(row.path);
    if (outcome) {
      return outcome.project ? (
        <span
          style={{ color: "var(--accent)", display: "inline-flex", gap: 4 }}
        >
          <CheckIcon style={{ width: 14, height: 14 }} /> Created
        </span>
      ) : (
        <span style={{ color: "#ef4444" }}>{outcome.error}</span>
      );
    }
    if (row.error) return <span style={{ color: "#ef4444" }}>{row.error}</span>;
    const notes: string[] = [];
    if (row.findingCount > 0) {
      notes.push(
        `${row.findingCount} issue${row.findingCount === 1 ? "" : "s"}`,
      );
    }
    if (row.repairs.length > 0) {
      notes.push(
        `${row.repairs.length} repair${
          row.repairs.length === 1 ? "" : "s"
        } on import`,
      );
    }
    if (notes.length === 0 && row.sidecars.length === 0) return null;
    return (
      <span
        style={{
          color: "var(--text-secondary)",
          display: "inline-flex",
          alignItems: "center",
          gap: 6,
        }}
      >
        {row.sidecars.length > 0 && (
          <span
            data-tooltip={`References ${row.sidecars.join(", ")} — not carried by this import; runs will refuse until the data is inlined`}
            style={{ display: "inline-flex", alignItems: "center" }}
          >
            <ExclamationTriangleIcon
              style={{ width: 14, height: 14, color: "#f59e0b" }}
            />
          </span>
        )}
        {notes.join(" · ")}
      </span>
    );
  }

  const done = outcomes !== null;
  const leftBehind = leftBehindSummary(scan.others);

  return (
    <ModalBackdrop onDismiss={onClose} zIndex={600}>
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby="import-archive-title"
        {...stopBackdropEvents}
        style={{
          background: "var(--bg-panel)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          padding: "22px 26px",
          width: "min(720px, calc(100vw - 48px))",
          maxHeight: "calc(100vh - 96px)",
          display: "flex",
          flexDirection: "column",
          gap: 14,
          boxShadow: "0 24px 64px rgba(0,0,0,0.4)",
        }}
      >
        <div>
          <p
            id="import-archive-title"
            style={{
              margin: 0,
              fontSize: "var(--text-xl)",
              fontWeight: 600,
              color: "var(--text-primary)",
            }}
          >
            Import models from archive
          </p>
          <p
            style={{
              margin: "4px 0 0",
              fontSize: "var(--text-md)",
              color: "var(--text-secondary)",
            }}
          >
            {done
              ? `${createdCount} of ${outcomes.size} selected model${
                  outcomes.size === 1 ? "" : "s"
                } became projects.`
              : "Each selected model becomes its own project."}
          </p>
        </div>

        <div style={{ overflowY: "auto", minHeight: 0, flexShrink: 1 }}>
          <table
            style={{ width: "100%", borderCollapse: "collapse" }}
            aria-label="Models found in the archive"
          >
            <thead>
              <tr>
                <th style={{ ...HEAD_CELL, width: 28 }} aria-label="Include" />
                <th style={HEAD_CELL}>Project name</th>
                <th style={HEAD_CELL}>Engine</th>
                <th style={{ ...HEAD_CELL, textAlign: "right" }}>Elements</th>
                <th style={HEAD_CELL}>Notes</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr key={row.path}>
                  <td style={CELL}>
                    <input
                      type="checkbox"
                      checked={row.include}
                      disabled={done || !rowImportable(row)}
                      aria-label={`Import ${row.path}`}
                      onChange={(e) =>
                        patchRow(row.path, { include: e.target.checked })
                      }
                    />
                  </td>
                  <td style={CELL}>
                    <input
                      type="text"
                      value={row.name}
                      disabled={done || !rowImportable(row)}
                      aria-label={`Project name for ${row.path}`}
                      onChange={(e) =>
                        patchRow(row.path, { name: e.target.value })
                      }
                      style={{
                        width: "100%",
                        boxSizing: "border-box",
                        background: "var(--bg-input, transparent)",
                        border: "1px solid var(--border)",
                        borderRadius: 6,
                        padding: "4px 8px",
                        fontSize: "var(--text-md)",
                        color: "var(--text-primary)",
                      }}
                    />
                    <div
                      style={{
                        fontSize: "var(--text-sm)",
                        color: "var(--text-secondary)",
                        marginTop: 2,
                        overflowWrap: "anywhere",
                      }}
                    >
                      {row.path}
                    </div>
                  </td>
                  <td style={{ ...CELL, whiteSpace: "nowrap" }}>
                    {row.engine !== null ? (
                      engineLabel(row.engine)
                    ) : row.candidates.length > 0 ? (
                      <select
                        value=""
                        disabled={done}
                        aria-label={`Engine for ${row.path}`}
                        onChange={(e) =>
                          setRows((prev) =>
                            withEngineChosen(prev, row.path, e.target.value),
                          )
                        }
                      >
                        <option value="" disabled>
                          Choose…
                        </option>
                        {row.candidates.map((key) => (
                          <option key={key} value={key}>
                            {engineLabel(key)}
                          </option>
                        ))}
                      </select>
                    ) : (
                      <span style={{ color: "var(--text-secondary)" }}>—</span>
                    )}
                  </td>
                  <td
                    style={{
                      ...CELL,
                      textAlign: "right",
                      whiteSpace: "nowrap",
                      color: "var(--text-secondary)",
                    }}
                  >
                    {rowImportable(row)
                      ? `${row.nodeCount} / ${row.linkCount}`
                      : "—"}
                  </td>
                  <td style={CELL}>{noteFor(row)}</td>
                </tr>
              ))}
              {rows.length === 0 && (
                <tr>
                  <td colSpan={5} style={{ ...CELL, textAlign: "center" }}>
                    No model files in this archive.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>

        {leftBehind && (
          <p
            style={{
              margin: 0,
              fontSize: "var(--text-sm)",
              color: "var(--text-secondary)",
            }}
          >
            {leftBehind}
          </p>
        )}
        {failure && (
          <p
            style={{ margin: 0, fontSize: "var(--text-md)", color: "#ef4444" }}
          >
            {failure}
          </p>
        )}

        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
          {!done && (
            <button
              type="button"
              onClick={onClose}
              style={{
                background: "transparent",
                border: "1px solid var(--border)",
                borderRadius: 6,
                padding: "6px 14px",
                fontSize: "var(--text-md)",
                fontWeight: 500,
                color: "var(--text-secondary)",
                cursor: "pointer",
              }}
            >
              Cancel
            </button>
          )}
          {done ? (
            <PrimaryButton onClick={() => onDone(createdCount)}>
              Done
            </PrimaryButton>
          ) : (
            <PrimaryButton
              onClick={() => void handleCreate()}
              disabled={busy || selections.length === 0}
            >
              {busy
                ? "Creating…"
                : `Create ${selections.length} project${
                    selections.length === 1 ? "" : "s"
                  }`}
            </PrimaryButton>
          )}
        </div>
      </div>
    </ModalBackdrop>
  );
}
