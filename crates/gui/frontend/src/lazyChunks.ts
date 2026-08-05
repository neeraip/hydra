/**
 * Loaders for code-split chunks that can be fetched *before* they are
 * needed.
 *
 * `React.lazy` starts its import on the first render that needs the
 * component, so a code-split overlay pays its download at the moment of
 * the click that opens it — with a `null` Suspense fallback, that reads as
 * the click not registering. Splitting is still right (this is code most
 * sessions never touch), so the fix is to move the fetch earlier rather
 * than to bundle it eagerly.
 *
 * Keeping the loader here rather than beside the component is what makes
 * that possible: whoever *triggers* the overlay can warm it without
 * importing it, which would defeat the split it is paying for.
 *
 * Calling a loader twice is free — the module registry returns the same
 * promise — so callers may fire on hover, on focus, and on idle without
 * coordinating.
 */

/**
 * The Settings drawer's *contents* — the rows, not the panel.
 *
 * The drawer's own chrome is imported eagerly: splitting it meant nothing
 * appeared until the chunk resolved, so the click looked ignored. Only what
 * fills an already-open drawer is worth deferring.
 */
export function loadSettingsContent() {
  return import("./components/modals/SettingsContent");
}

/**
 * Run `fn` when the browser is next idle, or shortly after paint where
 * that is unavailable.
 *
 * `requestIdleCallback` is missing from WebKit, which is the engine the
 * macOS build runs on — so the platform most likely to lack it is one we
 * ship to, and a bare call would silently never prefetch there.
 */
export function whenIdle(fn: () => void): () => void {
  if (typeof requestIdleCallback === "function") {
    const handle = requestIdleCallback(fn, { timeout: 2000 });
    return () => cancelIdleCallback(handle);
  }
  const handle = setTimeout(fn, 800);
  return () => clearTimeout(handle);
}
