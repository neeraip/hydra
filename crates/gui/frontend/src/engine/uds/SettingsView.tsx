/** The uds implementation of the settings modal body: the same read-only
 * summary the run modal shows, with the editing status stated plainly. */

import type { SettingsViewProps } from "../registry";
import { UdsRunSettingsSummary } from "./RunSettingsSummary";

export function UdsSettingsView({ projectId }: SettingsViewProps) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <UdsRunSettingsSummary projectId={projectId} />
      <div
        style={{
          fontSize: "var(--text-sm)",
          color: "var(--text-tertiary)",
          lineHeight: 1.5,
        }}
      >
        Read-only — editing these settings in the GUI is not available yet. Runs
        use the model file&rsquo;s settings as shown.
      </div>
    </div>
  );
}
