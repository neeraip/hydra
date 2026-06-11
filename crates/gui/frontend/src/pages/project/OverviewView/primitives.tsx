// ─── Reusable atoms ──────────────────────────────────────────────────────────

export function Section({
  title,
  tag,
  right,
  children,
}: {
  title: string;
  tag?: React.ReactNode;
  right?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <section
      style={{
        background: "var(--bg-card)",
        border: "1px solid var(--border)",
        borderRadius: 10,
        padding: 16,
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 12,
          gap: 8,
        }}
      >
        <div
          style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}
        >
          <h2
            style={{
              margin: 0,
              fontSize: 11,
              fontWeight: 600,
              color: "var(--text-tertiary)",
              textTransform: "uppercase",
              letterSpacing: "0.06em",
              whiteSpace: "nowrap",
            }}
          >
            {title}
          </h2>
          {tag}
        </div>
        {right != null && (
          <span
            style={{
              fontSize: 11,
              color: "var(--text-tertiary)",
              whiteSpace: "nowrap",
              flexShrink: 0,
            }}
          >
            {right}
          </span>
        )}
      </div>
      {children}
    </section>
  );
}

export function KpiGrid({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(4, minmax(0, 1fr))",
        gap: 10,
      }}
    >
      {children}
    </div>
  );
}

export function Kpi({
  label,
  value,
  sub,
  muted,
  warn,
}: {
  label: string;
  value: string;
  sub: string;
  muted?: boolean;
  warn?: boolean;
}) {
  return (
    <div
      style={{
        background: "var(--bg-panel)",
        border: `1px solid ${warn ? "rgba(212,160,23,0.30)" : "var(--border)"}`,
        borderRadius: 8,
        padding: "10px 12px",
        minWidth: 0,
      }}
    >
      <div
        style={{
          fontSize: 10,
          fontWeight: 600,
          letterSpacing: "0.06em",
          color: "var(--text-tertiary)",
          textTransform: "uppercase",
        }}
      >
        {label}
      </div>
      <div
        style={{
          marginTop: 4,
          fontSize: 18,
          fontWeight: 600,
          fontFamily: "var(--font-mono)",
          color: muted
            ? "var(--text-tertiary)"
            : warn
              ? "var(--status-warning)"
              : "var(--text-primary)",
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}
      >
        {value}
      </div>
      <div
        style={{
          marginTop: 4,
          fontSize: 11,
          color: warn ? "var(--status-warning)" : "var(--text-tertiary)",
          whiteSpace: "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}
      >
        {sub}
      </div>
    </div>
  );
}

export function EmptyState({
  message,
  ctaLabel,
  onCta,
}: {
  message: string;
  ctaLabel: string;
  onCta: () => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 12,
        padding: "10px 12px",
        background: "var(--bg-panel)",
        border: "1px dashed var(--border)",
        borderRadius: 8,
      }}
    >
      <span style={{ fontSize: 12, color: "var(--text-tertiary)" }}>
        {message}
      </span>
      <button
        onClick={onCta}
        onMouseEnter={(e) => {
          (e.currentTarget as HTMLButtonElement).style.background =
            "var(--accent)";
          (e.currentTarget as HTMLButtonElement).style.color = "#fff";
        }}
        onMouseLeave={(e) => {
          (e.currentTarget as HTMLButtonElement).style.background =
            "var(--accent-dim)";
          (e.currentTarget as HTMLButtonElement).style.color = "var(--accent)";
        }}
        style={{
          background: "var(--accent-dim)",
          color: "var(--accent)",
          border: "1px solid rgba(18, 54, 92, 0.2)",
          borderRadius: 6,
          padding: "5px 10px",
          fontSize: 12,
          fontWeight: 500,
          cursor: "pointer",
          fontFamily: "var(--font-ui)",
          whiteSpace: "nowrap",
          transition: "background var(--t-fast), color var(--t-fast)",
        }}
      >
        {ctaLabel}
      </button>
    </div>
  );
}

export function Dot({ color }: { color: string }) {
  return (
    <span
      style={{
        width: 8,
        height: 8,
        borderRadius: "50%",
        background: color,
        display: "inline-block",
        flexShrink: 0,
      }}
    />
  );
}

export function PrimaryButton({
  children,
  onClick,
}: {
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background =
          "var(--accent)";
        (e.currentTarget as HTMLButtonElement).style.color = "#fff";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background =
          "var(--accent-dim)";
        (e.currentTarget as HTMLButtonElement).style.color = "var(--accent)";
      }}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        background: "var(--accent-dim)",
        color: "var(--accent)",
        border: "1px solid rgba(18, 54, 92, 0.2)",
        borderRadius: 7,
        padding: "6px 12px",
        fontSize: 12,
        fontWeight: 500,
        cursor: "pointer",
        fontFamily: "var(--font-ui)",
        transition: "background var(--t-fast), color var(--t-fast)",
      }}
    >
      {children}
    </button>
  );
}

export function SecondaryButton({
  children,
  onClick,
}: {
  children: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background =
          "var(--nav-hover)";
        (e.currentTarget as HTMLButtonElement).style.color =
          "var(--text-primary)";
        (e.currentTarget as HTMLButtonElement).style.borderColor =
          "var(--border-hover)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background =
          "var(--bg-panel)";
        (e.currentTarget as HTMLButtonElement).style.color =
          "var(--text-secondary)";
        (e.currentTarget as HTMLButtonElement).style.borderColor =
          "var(--border)";
      }}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 5,
        background: "var(--bg-panel)",
        color: "var(--text-secondary)",
        border: "1px solid var(--border)",
        borderRadius: 7,
        padding: "6px 12px",
        fontSize: 12,
        fontWeight: 500,
        cursor: "pointer",
        fontFamily: "var(--font-ui)",
        transition:
          "background var(--t-fast), color var(--t-fast), border-color var(--t-fast)",
      }}
    >
      {children}
    </button>
  );
}

export function IconButton({
  children,
  onClick,
  title,
}: {
  children: React.ReactNode;
  onClick: () => void;
  title: string;
}) {
  return (
    <button
      onClick={onClick}
      data-tooltip={title}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background =
          "var(--nav-hover)";
        (e.currentTarget as HTMLButtonElement).style.borderColor =
          "var(--border-hover)";
        (e.currentTarget as HTMLButtonElement).style.color =
          "var(--text-primary)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.background =
          "var(--bg-panel)";
        (e.currentTarget as HTMLButtonElement).style.borderColor =
          "var(--border)";
        (e.currentTarget as HTMLButtonElement).style.color =
          "var(--text-secondary)";
      }}
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        background: "var(--bg-panel)",
        color: "var(--text-secondary)",
        border: "1px solid var(--border)",
        borderRadius: 7,
        padding: "6px 8px",
        cursor: "pointer",
        transition:
          "background var(--t-fast), border-color var(--t-fast), color var(--t-fast)",
      }}
    >
      {children}
    </button>
  );
}
