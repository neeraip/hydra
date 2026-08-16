// ── The race guard under every fetch-into-state effect ───────────────────────
//
// The idiom this replaces appeared forty-odd times: start one fetch in an
// effect, guard the setState behind a `cancelled` flag, flip the flag in
// cleanup so a slow answer cannot overwrite a newer question's. Six lines
// whose only variation was clerical, copied enough times that the copies
// had begun to differ by accident rather than intent.
//
// Deliberately *not* a hook. A hook would have to take the dependency
// array, and a custom hook's deps are invisible to
// `useExhaustiveDependencies` — which this codebase leans on: every
// intentional retrigger (`networkVersion`, `resultGeneration`) is
// documented at its call site with a suppression naming the reason. This
// stays a plain function returning the effect's cleanup, so the deps, the
// lint, and the reset-on-missing-argument all stay where they were.
//
// What it cannot express, it does not try to: a chain that checks for
// cancellation *between* steps, or a loading flag cleared in `finally`,
// is a different shape and stays hand-rolled.

/**
 * Start `promise` and apply its result, unless the effect has re-run or
 * unmounted first. Returns the cleanup the effect hands back.
 *
 * ```ts
 * useEffect(() => {
 *   if (!engine) {
 *     setKinds([]);
 *     return;
 *   }
 *   return fetchInto(getElementKinds(engine), setKinds);
 * }, [engine]);
 * ```
 *
 * Rejections are left to the promise the caller built — the fetchers
 * behind these effects resolve to a fallback rather than reject
 * (`tryInvokeOr`), so a rejection reaching this guard is a bug worth a
 * loud unhandled-rejection report rather than a silent swallow.
 */
export function fetchInto<T>(
  promise: Promise<T>,
  apply: (value: T) => void,
): () => void {
  let cancelled = false;
  void promise.then((value) => {
    if (!cancelled) apply(value);
  });
  return () => {
    cancelled = true;
  };
}
