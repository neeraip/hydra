/**
 * SidecarChecklist — the auxiliary files an imported model references,
 * each answered: included (found beside the model, or attached), or
 * missing with a way to locate it.
 *
 * A drainage model that reads its rain from a file is defunct without
 * that file — the import succeeds, the project opens, and every run
 * refuses. This list is how the wizard says so *before* the project is
 * created, when the fix is one file-picker away, rather than after, when
 * it is a mystery error in a run queue.
 */

import { CheckIcon, ExclamationTriangleIcon } from "@heroicons/react/16/solid";
import type { SidecarRef } from "../../hooks";

export function SidecarChecklist({
  sidecars,
  busy,
  onLocate,
}: {
  sidecars: SidecarRef[];
  /** A locate dialog is already open — the buttons wait for it. */
  busy: boolean;
  /** The user wants to point at a missing file on disk. */
  onLocate: () => void;
}) {
  if (sidecars.length === 0) return null;
  return (
    <div>
      <div
        style={{
          fontSize: "var(--text-sm)",
          fontWeight: 600,
          color: "var(--text-secondary)",
          marginBottom: 4,
        }}
      >
        Data files this model reads
      </div>
      <ul style={{ listStyle: "none", margin: 0, padding: 0 }}>
        {sidecars.map((sidecar) => (
          <li
            key={sidecar.file}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              padding: "3px 0",
              fontSize: "var(--text-md)",
              color: "var(--text-primary)",
              textAlign: "left",
            }}
          >
            {sidecar.carried && sidecar.supported ? (
              <CheckIcon
                aria-label="Included"
                style={{
                  width: 14,
                  height: 14,
                  flexShrink: 0,
                  color: "var(--status-success)",
                }}
              />
            ) : (
              <ExclamationTriangleIcon
                aria-label="Missing"
                style={{
                  width: 14,
                  height: 14,
                  flexShrink: 0,
                  color: "var(--status-warning)",
                }}
              />
            )}
            <span style={{ flex: 1, overflowWrap: "anywhere" }}>
              {sidecar.label}
            </span>
            {!sidecar.supported ? (
              <span
                style={{
                  fontSize: "var(--text-sm)",
                  color: "var(--status-warning)",
                  flexShrink: 0,
                }}
              >
                not supported yet
              </span>
            ) : sidecar.carried ? (
              <span
                style={{
                  fontSize: "var(--text-sm)",
                  color: "var(--status-success)",
                  flexShrink: 0,
                }}
              >
                will be imported
              </span>
            ) : (
              <button
                type="button"
                disabled={busy}
                onClick={onLocate}
                className="legend-picker-option"
                style={{ flexShrink: 0, width: "auto", padding: "2px 10px" }}
              >
                Locate…
              </button>
            )}
          </li>
        ))}
      </ul>
      {sidecars.some((s) => s.supported && !s.carried) && (
        <div
          style={{
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
            marginTop: 4,
          }}
        >
          Without these files the project imports, but simulations will refuse
          to run.
        </div>
      )}
    </div>
  );
}
