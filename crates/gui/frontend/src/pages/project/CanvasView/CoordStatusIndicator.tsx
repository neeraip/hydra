import {
  ExclamationCircleIcon,
  ExclamationTriangleIcon,
} from "@heroicons/react/16/solid";
import { useEffect, useRef, useState } from "react";

export function CoordStatusIndicator({
  status,
  missingCount,
  totalCount,
}: {
  status: "partial" | "empty";
  missingCount: number;
  totalCount: number;
}) {
  const [open, setOpen] = useState(false);
  const btnRef = useRef<HTMLButtonElement | null>(null);
  const popRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    function onDown(e: PointerEvent) {
      const t = e.target as Node | null;
      if (btnRef.current?.contains(t)) return;
      if (popRef.current?.contains(t)) return;
      setOpen(false);
    }
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  }, [open]);

  const isError = status === "empty";
  const accent = isError
    ? "var(--status-error, #c94040)"
    : "var(--status-warn, #c97810)";
  const bgDim = isError ? "rgba(201,64,64,0.13)" : "rgba(201,120,16,0.13)";
  const bgPop = isError ? "rgba(201,64,64,0.08)" : "rgba(201,120,16,0.08)";

  return (
    <div style={{ position: "relative" }}>
      <button
        type="button"
        ref={btnRef}
        className="tool-btn"
        onClick={() => setOpen((v) => !v)}
        data-tooltip={
          isError ? "No map coordinates" : "Partial map coordinates"
        }
        style={{
          width: 28,
          height: 28,
          background: bgDim,
          color: accent,
          border: `1px solid ${accent}`,
          flexShrink: 0,
        }}
      >
        {isError ? (
          <ExclamationCircleIcon style={{ width: 14, height: 14 }} />
        ) : (
          <ExclamationTriangleIcon style={{ width: 14, height: 14 }} />
        )}
      </button>

      {open && (
        <div
          ref={popRef}
          style={{
            position: "absolute",
            top: "calc(100% + 6px)",
            left: "50%",
            transform: "translateX(-50%)",
            minWidth: 280,
            maxWidth: 340,
            background: "var(--bg-panel)",
            border: `1px solid ${accent}`,
            borderRadius: 9,
            boxShadow: "var(--shadow-2)",
            zIndex: 30,
            padding: "12px 14px",
            display: "flex",
            flexDirection: "column",
            gap: 8,
          }}
        >
          {/* Header */}
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <div
              style={{
                width: 24,
                height: 24,
                borderRadius: "50%",
                background: bgPop,
                color: accent,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                flexShrink: 0,
              }}
            >
              {isError ? (
                <ExclamationCircleIcon style={{ width: 15, height: 15 }} />
              ) : (
                <ExclamationTriangleIcon style={{ width: 15, height: 15 }} />
              )}
            </div>
            <span
              style={{
                fontSize: 13,
                fontWeight: 700,
                color: "var(--text-primary)",
              }}
            >
              {isError ? "No map coordinates" : "Partial coordinates"}
            </span>
          </div>

          {/* Body */}
          <p
            style={{
              margin: 0,
              fontSize: 12,
              color: "var(--text-secondary)",
              lineHeight: 1.6,
            }}
          >
            {isError ? (
              <>
                None of the <strong>{totalCount}</strong> node
                {totalCount !== 1 ? "s" : ""} have a{" "}
                <code
                  style={{
                    fontFamily: "var(--font-mono)",
                    fontSize: 11,
                    background: bgPop,
                    padding: "1px 4px",
                    borderRadius: 3,
                  }}
                >
                  [COORDINATES]
                </code>{" "}
                entry. Map mode will be empty. Switch to{" "}
                <strong>Schematic</strong> to see the topology.
              </>
            ) : (
              <>
                <strong>{missingCount}</strong> of <strong>{totalCount}</strong>{" "}
                node{totalCount !== 1 ? "s" : ""} are missing a{" "}
                <code
                  style={{
                    fontFamily: "var(--font-mono)",
                    fontSize: 11,
                    background: bgPop,
                    padding: "1px 4px",
                    borderRadius: 3,
                  }}
                >
                  [COORDINATES]
                </code>{" "}
                entry and won't appear on the map.
              </>
            )}
          </p>

          {/* Dismiss */}
          <button
            type="button"
            onClick={() => setOpen(false)}
            style={{
              alignSelf: "flex-end",
              background: "transparent",
              border: "1px solid var(--border)",
              borderRadius: 5,
              color: "var(--text-secondary)",
              cursor: "pointer",
              fontSize: 11,
              padding: "3px 10px",
              fontFamily: "var(--font-ui)",
            }}
          >
            Dismiss
          </button>
        </div>
      )}
    </div>
  );
}
