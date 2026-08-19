import {
  ArrowTurnDownRightIcon,
  CheckIcon,
  PlayIcon,
  PlusIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import React from "react";
import { RowMenu } from "../../ui/RowMenu";
import {
  type FlatScenario,
  iconButtonStyle,
  rowButtonStyle,
  STATE_COLOR,
  STATE_LABEL,
} from "./shared";

export function BaseRow({
  isActive,
  onActivate,
  onNewScenario,
  canBranch,
  simulated,
  onClearResults,
  clearDetail,
  clearAllCount,
  onClearAllResults,
  clearAllDetail,
}: {
  isActive: boolean;
  onActivate: () => void;
  onNewScenario: () => void;
  /** False while the project has no network — there is nothing to branch. */
  canBranch: boolean;
  /** Whether the base model currently holds simulation results. */
  simulated: boolean;
  onClearResults: () => void;
  /** e.g. "Frees 12.4 MB" — what clearing the base model reclaims. */
  clearDetail?: string;
  /**
   * How many targets across the whole project hold results. Zero disables
   * the clear-all entry rather than removing it — inside a labelled menu an
   * inert row can say why it does not apply, which an absent row cannot.
   */
  clearAllCount: number;
  onClearAllResults: () => void;
  /** What a project-wide clear reclaims. */
  clearAllDetail?: string;
}) {
  // Mirrors the scenario rows' derivation so both read from one vocabulary.
  const baseState = simulated ? "simulated" : "not-run";
  const baseStateColor = STATE_COLOR[baseState] ?? "var(--text-tertiary)";
  const baseStateLabel = STATE_LABEL[baseState] ?? baseState;

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        padding: "12px 16px",
        gap: 12,
        borderBottom: "1px solid var(--border)",
        background: isActive ? "var(--selection-bg)" : undefined,
        transition: "background 0.15s",
      }}
    >
      <div
        style={{
          width: 3,
          alignSelf: "stretch",
          borderRadius: 2,
          background: isActive ? "var(--accent)" : "transparent",
          flexShrink: 0,
        }}
      />

      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span
            style={{
              fontSize: "var(--text-lg)",
              fontWeight: 600,
              color: isActive ? "var(--accent)" : "var(--text-primary)",
            }}
          >
            Base model
          </span>
          {isActive && (
            <span
              style={{
                fontSize: "var(--text-xs)",
                fontWeight: 600,
                color: "var(--accent)",
                background: "var(--selection-bg-strong)",
                borderRadius: 10,
                padding: "1px 7px",
              }}
            >
              Active
            </span>
          )}

          {/* The same state badge the scenario rows carry. The base model is
              a simulation target like any other, so reading its state must
              not require inferring it from which actions are available. */}
          <span
            style={{
              fontSize: "var(--text-xs)",
              fontWeight: 600,
              color: baseStateColor,
              background: `${baseStateColor}22`,
              borderRadius: 10,
              padding: "1px 7px",
            }}
          >
            {baseStateLabel}
          </span>
        </div>
        <div
          style={{
            fontSize: "var(--text-sm)",
            color: "var(--text-tertiary)",
            marginTop: 2,
          }}
        >
          {canBranch
            ? "Canonical network. All scenarios branch from here."
            : "No network yet. Import a model file or build one in the editor."}
        </div>
      </div>

      {!isActive && (
        <button
          type="button"
          onClick={onActivate}
          style={rowButtonStyle}
          data-tooltip="Switch to Base model"
        >
          Switch to Base
        </button>
      )}

      <button
        type="button"
        onClick={onNewScenario}
        disabled={!canBranch}
        style={{
          ...rowButtonStyle,
          display: "inline-flex",
          alignItems: "center",
          gap: 4,
          opacity: canBranch ? 1 : 0.45,
          cursor: canBranch ? "pointer" : "default",
        }}
        data-tooltip={
          canBranch
            ? "Create a new scenario branching from the base model"
            : "The base model has no network to branch from"
        }
      >
        <PlusIcon style={{ width: 10, height: 10 }} />
        New scenario
      </button>

      <RowMenu
        label="Base model actions"
        items={[
          {
            label: "Clear results",
            detail: clearDetail,
            onSelect: onClearResults,
            disabled: !simulated,
            disabledReason: "The base model has not been simulated",
            danger: true,
          },
          {
            label: "Clear all results",
            detail: clearAllDetail,
            onSelect: onClearAllResults,
            disabled: clearAllCount === 0,
            disabledReason: "Nothing in this project has been simulated",
            danger: true,
          },
        ]}
      />
    </div>
  );
}

export function ScenarioRow({
  scenario,
  isActive,
  isRenaming,
  renameValue,
  renameInputRef,
  isDeleting,
  isRunning,
  parentName,
  onActivate,
  onRenameStart,
  onRenameChange,
  onRenameCommit,
  onRenameCancel,
  onBranch,
  onRun,
  onResume,
  isResumable,
  onClearResults,
  clearDetail,
  onDelete,
  onOpenFolder,
}: {
  scenario: FlatScenario;
  isActive: boolean;
  isRenaming: boolean;
  renameValue: string;
  renameInputRef?: React.RefObject<HTMLInputElement | null>;
  isDeleting: boolean;
  isRunning: boolean;
  parentName: string | null;
  onActivate: () => void;
  onRenameStart: () => void;
  onRenameChange: (v: string) => void;
  onRenameCommit: () => void;
  onRenameCancel: () => void;
  onBranch: () => void;
  onRun: () => void;
  /** Continue this scenario's interrupted run from where it stopped. */
  onResume: () => void;
  /** Whether this scenario has an interrupted run to continue. */
  isResumable: boolean;
  onClearResults: () => void;
  /** e.g. "Frees 12.4 MB" — what clearing this scenario reclaims. */
  clearDetail?: string;
  onDelete: () => void;
  onOpenFolder: () => void;
}) {
  const stateColor = STATE_COLOR[scenario.state] ?? "var(--text-tertiary)";
  const stateLabel = STATE_LABEL[scenario.state] ?? scenario.state;
  // "stale" still means a results file exists — it is results that no longer
  // match the edited network, which is exactly when clearing is most useful.
  const hasResults =
    scenario.state === "simulated" || scenario.state === "stale";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        padding: "11px 16px",
        gap: 12,
        borderBottom: "1px solid var(--border)",
        background: isActive ? "var(--selection-bg)" : undefined,
        opacity: isDeleting ? 0.4 : 1,
        transition: "background 0.15s, opacity 0.15s",
      }}
    >
      {/* Active bar */}
      <div
        style={{
          width: 3,
          alignSelf: "stretch",
          borderRadius: 2,
          background: isActive ? "var(--accent)" : "transparent",
          flexShrink: 0,
        }}
      />

      {/* Tree indent */}
      {scenario.depth > 0 && (
        <div
          style={{
            flexShrink: 0,
            paddingLeft: (scenario.depth - 1) * 16,
            display: "flex",
            alignItems: "center",
          }}
        >
          <ArrowTurnDownRightIcon
            style={{
              width: 11,
              height: 11,
              color: "var(--text-tertiary)",
              marginRight: 4,
            }}
          />
        </div>
      )}

      {/* Name / rename field */}
      <div style={{ flex: 1, minWidth: 0 }}>
        {isRenaming ? (
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <input
              ref={renameInputRef as React.RefObject<HTMLInputElement>}
              value={renameValue}
              onChange={(e) => onRenameChange(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") onRenameCommit();
                if (e.key === "Escape") onRenameCancel();
              }}
              style={{
                fontSize: "var(--text-lg)",
                fontFamily: "var(--font-ui)",
                background: "var(--bg-input)",
                border: "1px solid var(--accent)",
                borderRadius: 4,
                color: "var(--text-primary)",
                padding: "3px 7px",
                outline: "none",
                width: 200,
              }}
            />
            <button
              type="button"
              onClick={onRenameCommit}
              style={iconButtonStyle}
              data-tooltip="Save"
              aria-label="Save name"
            >
              <CheckIcon style={{ width: 11, height: 11 }} />
            </button>
            <button
              type="button"
              onClick={onRenameCancel}
              style={iconButtonStyle}
              data-tooltip="Cancel"
              aria-label="Cancel rename"
            >
              <XMarkIcon style={{ width: 11, height: 11 }} />
            </button>
          </div>
        ) : (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              flexWrap: "wrap",
            }}
          >
            <span
              style={{
                fontSize: "var(--text-lg)",
                fontWeight: 500,
                color: isActive ? "var(--accent)" : "var(--text-primary)",
              }}
            >
              {scenario.name}
            </span>

            {isActive && (
              <span
                style={{
                  fontSize: "var(--text-xs)",
                  fontWeight: 600,
                  color: "var(--accent)",
                  background: "var(--selection-bg-strong)",
                  borderRadius: 10,
                  padding: "1px 7px",
                }}
              >
                Active
              </span>
            )}

            <span
              style={{
                fontSize: "var(--text-xs)",
                fontWeight: 600,
                color: stateColor,
                background: `${stateColor}22`,
                borderRadius: 10,
                padding: "1px 7px",
              }}
            >
              {stateLabel}
            </span>
          </div>
        )}

        {!isRenaming && parentName !== null && (
          <div
            style={{
              fontSize: "var(--text-sm)",
              color: "var(--text-tertiary)",
              marginTop: 2,
            }}
          >
            Branched from{" "}
            <span style={{ color: "var(--text-secondary)" }}>{parentName}</span>
          </div>
        )}
      </div>

      {/* Action buttons */}
      {!isRenaming && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 4,
            flexShrink: 0,
          }}
        >
          {!isActive && (
            <button
              type="button"
              onClick={onActivate}
              style={rowButtonStyle}
              data-tooltip="Switch to this scenario"
            >
              Switch
            </button>
          )}

          <button
            type="button"
            onClick={onRun}
            disabled={isRunning}
            style={{
              ...iconButtonStyle,
              color: isRunning ? "var(--text-tertiary)" : "#7bbf95",
            }}
            data-tooltip="Run simulation"
            aria-label="Run simulation"
          >
            <PlayIcon style={{ width: 12, height: 12 }} />
          </button>

          <RowMenu
            label={`Actions for ${scenario.name}`}
            items={[
              { label: "Branch from this scenario", onSelect: onBranch },
              {
                label: "Continue interrupted run",
                onSelect: onResume,
                disabled: !isResumable,
                disabledReason: "This scenario has no interrupted run",
              },
              { label: "Rename…", onSelect: onRenameStart },
              { label: "Open in Finder", onSelect: onOpenFolder },
              {
                label: "Clear results",
                detail: clearDetail,
                onSelect: onClearResults,
                disabled: !hasResults,
                disabledReason: "This scenario has not been simulated",
                danger: true,
              },
              {
                label: "Delete scenario",
                onSelect: onDelete,
                disabled: isDeleting,
                danger: true,
              },
            ]}
          />
        </div>
      )}
    </div>
  );
}

export const CreateRow = React.forwardRef<
  HTMLInputElement,
  {
    value: string;
    parentName: string | null;
    indent?: number;
    onChange: (v: string) => void;
    onCommit: () => void;
    onCancel: () => void;
  }
>(function CreateRow(
  { value, parentName, indent = 0, onChange, onCommit, onCancel },
  ref,
) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        padding: "10px 16px",
        gap: 10,
        borderBottom: "1px solid var(--border)",
        background: "var(--bg-hover, rgba(255,255,255,0.03))",
      }}
    >
      <div style={{ width: 3, flexShrink: 0 }} />
      {indent > 0 && (
        <div
          style={{
            paddingLeft: (indent - 1) * 16,
            display: "flex",
            alignItems: "center",
            flexShrink: 0,
          }}
        >
          <ArrowTurnDownRightIcon
            style={{
              width: 11,
              height: 11,
              color: "var(--text-tertiary)",
              marginRight: 4,
            }}
          />
        </div>
      )}

      <PlusIcon
        style={{
          width: 12,
          height: 12,
          color: "var(--text-tertiary)",
          flexShrink: 0,
        }}
      />

      <input
        ref={ref}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") onCommit();
          if (e.key === "Escape") onCancel();
        }}
        placeholder={
          parentName
            ? `Branch name from "${parentName}"…`
            : "New scenario name…"
        }
        style={{
          flex: 1,
          fontSize: "var(--text-lg)",
          fontFamily: "var(--font-ui)",
          background: "var(--bg-input)",
          border: "1px solid var(--border-hover)",
          borderRadius: 4,
          color: "var(--text-primary)",
          padding: "4px 8px",
          outline: "none",
        }}
      />

      <button type="button" onClick={onCommit} style={rowButtonStyle}>
        Create
      </button>
      <button
        type="button"
        onClick={onCancel}
        style={iconButtonStyle}
        data-tooltip="Cancel"
        aria-label="Cancel new scenario"
      >
        <XMarkIcon style={{ width: 12, height: 12 }} />
      </button>
    </div>
  );
});
