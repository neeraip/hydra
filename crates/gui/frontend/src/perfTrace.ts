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

export function isPerfTraceEnabled(): boolean {
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
