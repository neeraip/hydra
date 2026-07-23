import { ArrowRightIcon, FolderOpenIcon } from "@heroicons/react/16/solid";
import { PILL } from "../../../hooks";
import { Dot, IconButton, PrimaryButton, SecondaryButton } from "./primitives";

export function Header({
  project,
  accent,
  onOpenCanvas,
  onOpenEditor,
  onOpenAnalysis,
  onOpenFolder,
}: {
  project: { name: string; state: string; scenarioCount: number };
  accent: string;
  onOpenCanvas: () => void;
  onOpenEditor: () => void;
  onOpenAnalysis: () => void;
  onOpenFolder: () => void;
}) {
  const stateColor =
    project.state === "simulated"
      ? "var(--status-success)"
      : project.state === "running"
        ? "var(--accent)"
        : project.state === "failed"
          ? "var(--status-error)"
          : "var(--text-tertiary)";
  const stateLabel =
    project.state === "simulated"
      ? "Simulated"
      : project.state === "running"
        ? "Running"
        : project.state === "failed"
          ? "Failed"
          : project.state === "ready"
            ? "Ready"
            : project.state === "stale"
              ? "Edited"
              : "Draft";

  return (
    <div
      style={{
        display: "flex",
        alignItems: "flex-start",
        justifyContent: "space-between",
        gap: 16,
      }}
    >
      <div style={{ minWidth: 0 }}>
        <h1
          style={{
            margin: 0,
            fontSize: 22,
            fontWeight: 600,
            color: "var(--text-primary)",
            lineHeight: 1.3,
            overflow: "hidden",
            textOverflow: "ellipsis",
            whiteSpace: "nowrap",
          }}
        >
          {project.name}
        </h1>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            marginTop: 6,
            flexWrap: "wrap",
          }}
        >
          <span
            style={{
              fontSize: 10,
              fontWeight: 700,
              letterSpacing: "0.06em",
              padding: "2px 7px",
              borderRadius: 4,
              background: `${accent}26`,
              border: `1px solid ${accent}55`,
              color: accent,
            }}
          >
            {PILL}
          </span>
          <Dot color={stateColor} />
          <span style={{ fontSize: 12, color: stateColor }}>{stateLabel}</span>
          <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
            · {project.scenarioCount} scenario
            {project.scenarioCount !== 1 ? "s" : ""}
          </span>
        </div>
      </div>

      <div style={{ display: "flex", gap: 6, flexShrink: 0 }}>
        <IconButton title="Open project folder" onClick={onOpenFolder}>
          <FolderOpenIcon style={{ width: 14, height: 14 }} />
        </IconButton>
        <PrimaryButton onClick={onOpenCanvas}>
          Canvas <ArrowRightIcon style={{ width: 13, height: 13 }} />
        </PrimaryButton>
        <SecondaryButton onClick={onOpenEditor}>
          Editor <ArrowRightIcon style={{ width: 13, height: 13 }} />
        </SecondaryButton>
        <SecondaryButton onClick={onOpenAnalysis}>
          Analysis <ArrowRightIcon style={{ width: 13, height: 13 }} />
        </SecondaryButton>
      </div>
    </div>
  );
}
