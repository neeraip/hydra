export function ProjectInfo({
  crs,
  modifiedLabel,
  lastRunLabel,
  engineLabel,
  onEditCrs,
  onOpenFolder,
}: {
  crs: string;
  modifiedLabel: string;
  lastRunLabel: string | null;
  engineLabel: string;
  onEditCrs: () => void;
  onOpenFolder: () => void;
}) {
  const rows: Array<{
    label: string;
    value: string;
    mono?: boolean;
    action?: () => void;
    actionLabel?: string;
  }> = [
    { label: "Engine", value: engineLabel },
    {
      label: "CRS",
      value: crs,
      mono: true,
      action: onEditCrs,
      actionLabel: "Change...",
    },
    { label: "Modified", value: modifiedLabel },
    { label: "Last run", value: lastRunLabel ?? "Never" },
    { label: "Bundle", value: "Open in Finder", action: onOpenFolder },
  ];
  return (
    <div>
      {rows.map((r, i) => (
        <div
          key={r.label}
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "baseline",
            padding: "8px 0",
            gap: 12,
            borderTop: i === 0 ? "none" : "1px solid var(--border)",
          }}
        >
          <span
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              textTransform: "uppercase",
              letterSpacing: "0.05em",
            }}
          >
            {r.label}
          </span>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 10,
              minWidth: 0,
            }}
          >
            <span
              style={{
                fontSize: 12,
                color: "var(--text-primary)",
                fontFamily: r.mono ? "var(--font-mono)" : "var(--font-ui)",
                textAlign: "right",
                overflow: "hidden",
                textOverflow: "ellipsis",
                whiteSpace: "nowrap",
                minWidth: 0,
              }}
            >
              {r.value}
            </span>
            {r.action && (
              <button
                type="button"
                onClick={r.action}
                onMouseEnter={(e) => {
                  (e.currentTarget as HTMLButtonElement).style.opacity = "0.7";
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLButtonElement).style.opacity = "1";
                }}
                style={{
                  background: "transparent",
                  border: "none",
                  padding: 0,
                  fontSize: 12,
                  color: "var(--accent)",
                  cursor: "pointer",
                  fontFamily: "var(--font-ui)",
                  transition: "opacity var(--t-fast)",
                  flexShrink: 0,
                }}
              >
                {r.actionLabel ?? "Open"}
              </button>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}
