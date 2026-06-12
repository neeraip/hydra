import {
  ArrowTurnDownRightIcon,
  CheckIcon,
  FolderOpenIcon,
  PencilIcon,
  PlayIcon,
  PlusIcon,
  TrashIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import React from "react";
import {
  type FlatScenario,
  iconButtonStyle,
  rowButtonStyle,
  STATE_COLOR,
  STATE_LABEL,
} from "./shared";

export function BaseRow({
  isActive,
  accent,
  onActivate,
  onNewScenario,
}: {
  isActive: boolean;
  accent: string;
  onActivate: () => void;
  onNewScenario: () => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        padding: "12px 16px",
        gap: 12,
        borderBottom: "1px solid var(--border)",
        background: isActive ? `${accent}0d` : undefined,
        transition: "background 0.15s",
      }}
    >
      <div
        style={{
          width: 3,
          alignSelf: "stretch",
          borderRadius: 2,
          background: isActive ? accent : "transparent",
          flexShrink: 0,
        }}
      />

      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span
            style={{
              fontSize: 13,
              fontWeight: 600,
              color: isActive ? accent : "var(--text-primary)",
            }}
          >
            Base model
          </span>
          {isActive && (
            <span
              style={{
                fontSize: 10,
                fontWeight: 600,
                color: accent,
                background: `${accent}22`,
                borderRadius: 10,
                padding: "1px 7px",
              }}
            >
              Active
            </span>
          )}
        </div>
        <div
          style={{ fontSize: 11, color: "var(--text-tertiary)", marginTop: 2 }}
        >
          Canonical network — all scenarios branch from here
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
        style={{
          ...rowButtonStyle,
          display: "inline-flex",
          alignItems: "center",
          gap: 4,
        }}
        data-tooltip="Create a new scenario branching from the base model"
      >
        <PlusIcon style={{ width: 10, height: 10 }} />
        New scenario
      </button>
    </div>
  );
}

export function ScenarioRow({
  scenario,
  isActive,
  accent,
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
  onDelete,
  onOpenFolder,
}: {
  scenario: FlatScenario;
  isActive: boolean;
  accent: string;
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
  onDelete: () => void;
  onOpenFolder: () => void;
}) {
  const stateColor = STATE_COLOR[scenario.state] ?? "var(--text-tertiary)";
  const stateLabel = STATE_LABEL[scenario.state] ?? scenario.state;

  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        padding: "11px 16px",
        gap: 12,
        borderBottom: "1px solid var(--border)",
        background: isActive ? `${accent}0d` : undefined,
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
          background: isActive ? accent : "transparent",
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
                fontSize: 13,
                fontFamily: "var(--font-ui)",
                background: "var(--bg-input)",
                border: `1px solid ${accent}`,
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
            >
              <CheckIcon style={{ width: 11, height: 11 }} />
            </button>
            <button
              type="button"
              onClick={onRenameCancel}
              style={iconButtonStyle}
              data-tooltip="Cancel"
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
                fontSize: 13,
                fontWeight: 500,
                color: isActive ? accent : "var(--text-primary)",
              }}
            >
              {scenario.name}
            </span>

            {isActive && (
              <span
                style={{
                  fontSize: 10,
                  fontWeight: 600,
                  color: accent,
                  background: `${accent}22`,
                  borderRadius: 10,
                  padding: "1px 7px",
                }}
              >
                Active
              </span>
            )}

            <span
              style={{
                fontSize: 10,
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
              fontSize: 11,
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
          >
            <PlayIcon style={{ width: 12, height: 12 }} />
          </button>

          <button
            type="button"
            onClick={onBranch}
            style={iconButtonStyle}
            data-tooltip="Branch from this scenario"
          >
            <ArrowTurnDownRightIcon style={{ width: 12, height: 12 }} />
          </button>

          <button
            type="button"
            onClick={onRenameStart}
            style={iconButtonStyle}
            data-tooltip="Rename"
          >
            <PencilIcon style={{ width: 12, height: 12 }} />
          </button>

          <button
            type="button"
            onClick={onOpenFolder}
            style={iconButtonStyle}
            data-tooltip="Open in Finder"
          >
            <FolderOpenIcon style={{ width: 12, height: 12 }} />
          </button>

          <button
            type="button"
            onClick={onDelete}
            disabled={isDeleting}
            style={{
              ...iconButtonStyle,
              color: "var(--status-error, #e05c5c)",
            }}
            data-tooltip="Delete scenario"
          >
            <TrashIcon style={{ width: 12, height: 12 }} />
          </button>
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
          fontSize: 13,
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
      >
        <XMarkIcon style={{ width: 12, height: 12 }} />
      </button>
    </div>
  );
});
