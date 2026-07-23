const PERF_TRACE_SUPPORTED = import.meta.env.DEV;

function nowMs(): number {
  if (
    typeof performance !== "undefined" &&
    typeof performance.now === "function"
  ) {
    return performance.now();
  }
  return Date.now();
}

function isPerfTraceEnabled(): boolean {
  return PERF_TRACE_SUPPORTED;
}

export function perfTrace(
  label: string,
  durationMs: number,
  data?: Record<string, unknown>,
): void {
  if (!isPerfTraceEnabled()) return;
  const roundedMs = Number(durationMs.toFixed(2));
  const payload = {
    label,
    durationMs: roundedMs,
    ...(data ?? {}),
  };
  // Keep logs structured for easy copy/paste into issue reports.
  console.info(`[hydra-perf] ${label} ${roundedMs}ms`, payload);
}

export function startPerfSpan(label: string, data?: Record<string, unknown>) {
  const start = nowMs();
  return {
    end(extra?: Record<string, unknown>) {
      perfTrace(label, nowMs() - start, { ...(data ?? {}), ...(extra ?? {}) });
    },
  };
}

/**
 * Dev-only main-thread stall watchdog: samples requestAnimationFrame and
 * logs a `main-thread-stall` perf trace whenever consecutive frames are more
 * than `thresholdMs` apart — the direct signature of a long synchronous task
 * blocking the UI. No-op (and never schedules a frame) outside dev builds.
 *
 * Returns a stop function.
 */
export function startMainThreadStallWatch(thresholdMs = 250): () => void {
  if (!isPerfTraceEnabled()) return () => {};
  if (typeof requestAnimationFrame !== "function") return () => {};
  let last = nowMs();
  let rafId = 0;
  let stopped = false;
  const tick = () => {
    if (stopped) return;
    const now = nowMs();
    const gap = now - last;
    // Occlusion/minimise pauses rAF entirely — a huge gap while hidden is
    // not a stall, so skip logging when the document wasn't visible.
    if (gap > thresholdMs && document.visibilityState === "visible") {
      perfTrace("main-thread-stall", gap, { at: Math.round(last) });
    }
    last = now;
    rafId = requestAnimationFrame(tick);
  };
  rafId = requestAnimationFrame(tick);
  return () => {
    stopped = true;
    cancelAnimationFrame(rafId);
  };
}
