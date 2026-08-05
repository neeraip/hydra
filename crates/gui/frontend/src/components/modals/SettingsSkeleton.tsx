/**
 * What the Settings drawer shows while its rows are loading.
 *
 * Built from the same `Section` and `SettingRow` the real content uses, so
 * the headings, paddings, rules and type sizes are not a copy of the real
 * measurements but the measurements themselves. A skeleton that restates
 * them drifts from what it stands in for, and the drift appears as exactly
 * the jump the skeleton exists to prevent.
 *
 * Labels are real text rather than grey bars. They are static strings the
 * loading state knows as well as the loaded one, and a reader who can
 * already see "Text size" arriving knows the drawer opened on the right
 * thing — a row of grey bars only says "something is coming".
 *
 * Only the controls are unknowable ahead of time, so only they shimmer.
 */

import { Section, SettingRow } from "./SettingsPrimitives";

/** Width of the placeholder standing in for each kind of control. */
const CONTROL_WIDTH = {
  toggle: 34,
  select: 128,
  buttons: 190,
  link: 96,
} as const;

function Placeholder({ width }: { width: number }) {
  return (
    <span
      aria-hidden
      style={{
        display: "block",
        width,
        height: 22,
        borderRadius: 6,
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        // Reuses the app's existing pulse rather than adding a shimmer
        // keyframe for one loading state.
        animation: "pulseDot 1.6s ease-in-out infinite",
      }}
    />
  );
}

/**
 * The rows this mirrors, in order.
 *
 * Duplicated from the content, and deliberately so: making the real rows
 * data-driven to share one list would flatten a set of bespoke controls
 * into a lowest common denominator, which is a worse trade than a list
 * that can fall one row out of date in a state that lasts 200ms. Being
 * wrong here costs a row of height for an instant; being wrong the other
 * way costs the settings themselves.
 */
const SECTIONS: Array<{
  title: string;
  rows: Array<{ label: string; control: keyof typeof CONTROL_WIDTH }>;
}> = [
  {
    title: "General",
    rows: [{ label: "Reopen last project on launch", control: "toggle" }],
  },
  {
    title: "Appearance",
    rows: [
      { label: "Theme", control: "buttons" },
      { label: "Default display units", control: "select" },
      { label: "Basemap providers", control: "link" },
    ],
  },
  {
    title: "Accessibility",
    rows: [
      { label: "Text size", control: "select" },
      { label: "Reduce motion", control: "toggle" },
      { label: "High-contrast mode", control: "toggle" },
    ],
  },
  {
    title: "About",
    rows: [{ label: "Data folder", control: "link" }],
  },
];

export function SettingsSkeleton() {
  return (
    <>
      {SECTIONS.map((section) => (
        <div key={section.title}>
          <Section>{section.title}</Section>
          {section.rows.map((row) => (
            <SettingRow key={row.label} label={row.label}>
              <Placeholder width={CONTROL_WIDTH[row.control]} />
            </SettingRow>
          ))}
        </div>
      ))}
    </>
  );
}
