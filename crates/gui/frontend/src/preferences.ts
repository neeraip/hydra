/**
 * Every preference the app keeps in local storage, and what "reset" means.
 *
 * The settings live in seventeen-odd keys written by the module that owns
 * each one, which is right — a preference belongs beside the thing it
 * governs. What was missing is anywhere that knows the whole set, so
 * "put it back how it was" was not an offer the app could make.
 *
 * The lists below are that place. They are exhaustive by test rather than
 * by intention: `preferences.test.ts` reads the source for every
 * `hydra2-` key and fails when one is not classified here, because the
 * failure mode of a hand-maintained list is a preference that quietly
 * survives a reset — or, worse, a piece of state that quietly does not.
 */

/** The rows in the Settings drawer. */
export const SETTINGS_KEYS = [
  "hydra2-theme",
  "hydra2-text-scale",
  "hydra2-unit-system",
  "hydra2-reduced-motion",
  "hydra2-high-contrast",
  "hydra2-restore-session",
  "hydra2-auto-update-check",
] as const;

/**
 * Preferences set by using the app rather than by visiting Settings: how
 * wide a rail was dragged, whether the canvas animates, which basemap
 * styles were hidden. A reader asking for their preferences back means
 * these too — they are the ones that accumulate without ever being
 * chosen.
 */
export const VIEW_KEYS = [
  "hydra2-link-animation",
  "hydra2-rail-open",
  "hydra2-rail-width",
  "hydra2-rail-dim-offscreen",
  "hydra2-basemap-visibility",
  "hydra2-basemap-hidden-styles",
] as const;

/**
 * Local storage that is not a preference, and survives a reset.
 *
 * Each for its own reason: two are *work* (which project to reopen, which
 * format a report was last exported as), two are bookkeeping the app
 * refills by itself, and one is a development marker whose whole purpose
 * is to persist while it is being used.
 */
export const KEPT_KEYS = [
  "hydra2-last-project",
  "hydra2-last-seen-gui-version",
  "hydra2-gui-releases-cache",
  "hydra2-updater-mock",
] as const;

/**
 * Families stored one key per project, matched by prefix.
 *
 * All of them are answers about a particular project rather than about
 * how the app should behave — which view you were in, what the canvas was
 * showing, which format you last exported. Resetting your preferences
 * should not walk through every project you own changing things.
 *
 * `hydra2-rail-open:` is the exception and is here for a different
 * reason: it is dead. The rail preference became one key for the whole
 * app, and the old per-project entries are read by nothing (see
 * `AppContext`). Removing them would be tidying storage nobody looks at,
 * under a button that promises something else.
 */
export const KEPT_PREFIXES = [
  "hydra2-project-view:",
  "hydra2-canvas-prefs:",
  "hydra2-report-format:",
  "hydra2-rail-open:",
];

/**
 * Put every preference back to its default.
 *
 * Removal rather than rewriting: each owning module already reads an
 * absent key as its default, so deleting is the one operation that cannot
 * disagree with them about what the default is.
 *
 * Returns how many keys were actually present, which is what the caller
 * reports back — "nothing to reset" and "everything reset" should not
 * look the same.
 */
export function resetPreferences(): number {
  let cleared = 0;
  for (const key of [...SETTINGS_KEYS, ...VIEW_KEYS]) {
    try {
      if (localStorage.getItem(key) !== null) cleared += 1;
      localStorage.removeItem(key);
    } catch {
      // A storage that cannot be written cannot be holding preferences
      // either; there is nothing to report and nothing to fail over.
    }
  }
  return cleared;
}
