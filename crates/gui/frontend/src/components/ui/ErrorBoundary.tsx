import {
  ArrowPathIcon,
  ClipboardDocumentIcon,
  ExclamationTriangleIcon,
} from "@heroicons/react/16/solid";
import { Component, type ErrorInfo, type ReactNode } from "react";

// ── User-friendly error translation ──────────────────────────────────────────

function friendlyMessage(error: Error): { title: string; detail: string } {
  const msg = error.message ?? "";

  if (
    msg.includes("is not a function") ||
    msg.includes("is not a constructor")
  ) {
    return {
      title: "A component failed to initialize",
      detail:
        "A piece of the interface tried to use something that wasn't available. " +
        "This is usually caused by a misconfigured feature or a missing data dependency.",
    };
  }

  if (
    msg.includes("Cannot read properties of undefined") ||
    msg.includes("Cannot read properties of null") ||
    msg.includes("is not an object") ||
    msg.includes("undefined is not an object")
  ) {
    return {
      title: "Data wasn't ready when the page loaded",
      detail:
        "A part of the page tried to display information before it finished loading. " +
        "This can happen when opening a view while a simulation or file operation is still in progress.",
    };
  }

  if (
    msg.includes("NetworkError") ||
    msg.includes("Failed to fetch") ||
    msg.includes("Load failed")
  ) {
    return {
      title: "A network request failed",
      detail:
        "The application could not reach the backend service. " +
        "Check that the app is running correctly and try reloading.",
    };
  }

  if (msg.includes("ChunkLoadError") || msg.includes("Loading chunk")) {
    return {
      title: "A page module failed to load",
      detail:
        "Part of the interface could not be downloaded. " +
        "This sometimes happens after an update. Reloading the app should resolve it.",
    };
  }

  return {
    title: "An unexpected error occurred",
    detail:
      "Something went wrong while rendering this part of the application. " +
      "You can try reloading, or navigate to a different section of the app.",
  };
}

// ── Component ─────────────────────────────────────────────────────────────────

interface Props {
  children: ReactNode;
  /**
   * Optional scope label shown in the error screen header, e.g. "Canvas" or
   * "Project Overview". Defaults to "Application".
   */
  scope?: string;
}

interface State {
  error: Error | null;
  info: ErrorInfo | null;
  copied: boolean;
  devExpanded: boolean;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, info: null, copied: false, devExpanded: false };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.setState({ error, info });
    // In production you could send this to a logging service here.
  }

  private handleReload() {
    this.setState({
      error: null,
      info: null,
      copied: false,
      devExpanded: false,
    });
  }

  private handleCopy() {
    const { error, info } = this.state;
    const text = [
      `Error: ${error?.message ?? "unknown"}`,
      "",
      error?.stack ?? "",
      "",
      "Component stack:",
      info?.componentStack ?? "",
    ].join("\n");
    navigator.clipboard.writeText(text).then(() => {
      this.setState({ copied: true });
      setTimeout(() => this.setState({ copied: false }), 2000);
    });
  }

  render() {
    const { error, info, copied, devExpanded } = this.state;
    if (!error) return this.props.children;

    const isDev = import.meta.env.DEV;
    const scope = this.props.scope ?? "Application";
    const { title, detail } = friendlyMessage(error);

    return (
      <div
        role="alert"
        style={{
          flex: 1,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          padding: "48px 32px",
          background: "var(--bg-app, #0b0b0e)",
          color: "var(--text-primary, #e8e8ec)",
          fontFamily: "var(--font-ui, system-ui, sans-serif)",
          minHeight: 0,
          overflow: "auto",
        }}
      >
        {/* Icon */}
        <div
          style={{
            width: 56,
            height: 56,
            borderRadius: "50%",
            background: "rgba(201, 64, 64, 0.12)",
            border: "1px solid rgba(201, 64, 64, 0.3)",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            marginBottom: 24,
            flexShrink: 0,
          }}
        >
          <ExclamationTriangleIcon
            style={{ width: 26, height: 26, color: "#c94040" }}
          />
        </div>

        {/* Header */}
        <div style={{ textAlign: "center", maxWidth: 480, marginBottom: 32 }}>
          <div
            style={{
              fontSize: 11,
              fontWeight: 600,
              letterSpacing: "0.08em",
              textTransform: "uppercase",
              color: "#c94040",
              marginBottom: 10,
            }}
          >
            {scope} Error
          </div>
          <h1
            style={{
              fontSize: 20,
              fontWeight: 700,
              color: "var(--text-primary, #e8e8ec)",
              margin: "0 0 12px",
              lineHeight: 1.3,
            }}
          >
            {title}
          </h1>
          <p
            style={{
              fontSize: 13,
              color: "var(--text-secondary, #9898a6)",
              lineHeight: 1.7,
              margin: 0,
            }}
          >
            {detail}
          </p>
        </div>

        {/* Actions */}
        <div
          style={{
            display: "flex",
            gap: 10,
            flexWrap: "wrap",
            justifyContent: "center",
            marginBottom: 32,
          }}
        >
          <button
            type="button"
            onClick={() => this.handleReload()}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 7,
              padding: "8px 18px",
              borderRadius: 6,
              border: "1px solid rgba(201,64,64,0.35)",
              background: "rgba(201,64,64,0.1)",
              color: "#e07070",
              fontSize: 13,
              fontFamily: "inherit",
              cursor: "pointer",
              fontWeight: 500,
            }}
          >
            <ArrowPathIcon style={{ width: 14, height: 14 }} />
            Try again
          </button>
          <button
            type="button"
            onClick={() => this.handleCopy()}
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 7,
              padding: "8px 18px",
              borderRadius: 6,
              border: "1px solid var(--border, rgba(255,255,255,0.1))",
              background: "transparent",
              color: copied
                ? "var(--text-secondary, #9898a6)"
                : "var(--text-tertiary, #6e6e7c)",
              fontSize: 13,
              fontFamily: "inherit",
              cursor: "pointer",
            }}
          >
            <ClipboardDocumentIcon style={{ width: 14, height: 14 }} />
            {copied ? "Copied!" : "Copy error details"}
          </button>
        </div>

        {/* Dev-mode error details */}
        {isDev && (
          <div
            style={{
              width: "100%",
              maxWidth: 700,
              background: "rgba(0,0,0,0.35)",
              border: "1px solid rgba(201,64,64,0.25)",
              borderRadius: 8,
              overflow: "hidden",
            }}
          >
            {/* Collapsible toggle */}
            <button
              type="button"
              onClick={() =>
                this.setState((s) => ({ devExpanded: !s.devExpanded }))
              }
              style={{
                width: "100%",
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                padding: "10px 16px",
                background: "transparent",
                border: "none",
                color: "#c94040",
                fontSize: 11,
                fontWeight: 600,
                letterSpacing: "0.07em",
                textTransform: "uppercase",
                cursor: "pointer",
                fontFamily: "var(--font-mono, monospace)",
                textAlign: "left",
              }}
            >
              <span>Developer details</span>
              <span style={{ fontSize: 14, lineHeight: 1 }}>
                {devExpanded ? "▲" : "▼"}
              </span>
            </button>

            {devExpanded && (
              <div
                style={{
                  borderTop: "1px solid rgba(201,64,64,0.2)",
                  padding: 16,
                }}
              >
                {/* Error message */}
                <div
                  style={{
                    fontFamily: "var(--font-mono, monospace)",
                    fontSize: 12,
                    color: "#e07070",
                    marginBottom: 12,
                    wordBreak: "break-all",
                  }}
                >
                  <span style={{ color: "#9898a6", marginRight: 8 }}>
                    Error:
                  </span>
                  {error.message}
                </div>

                {/* Stack trace */}
                {error.stack && (
                  <>
                    <div
                      style={{
                        fontSize: 10,
                        fontWeight: 600,
                        letterSpacing: "0.07em",
                        textTransform: "uppercase",
                        color: "var(--text-disabled, #4e4e5a)",
                        marginBottom: 6,
                      }}
                    >
                      Stack trace
                    </div>
                    <pre
                      style={{
                        fontFamily: "var(--font-mono, monospace)",
                        fontSize: 11,
                        color: "var(--text-tertiary, #6e6e7c)",
                        margin: 0,
                        whiteSpace: "pre-wrap",
                        wordBreak: "break-all",
                        lineHeight: 1.6,
                        maxHeight: 200,
                        overflow: "auto",
                        background: "rgba(0,0,0,0.2)",
                        borderRadius: 4,
                        padding: "10px 12px",
                        marginBottom: 12,
                      }}
                    >
                      {error.stack}
                    </pre>
                  </>
                )}

                {/* Component stack */}
                {info?.componentStack && (
                  <>
                    <div
                      style={{
                        fontSize: 10,
                        fontWeight: 600,
                        letterSpacing: "0.07em",
                        textTransform: "uppercase",
                        color: "var(--text-disabled, #4e4e5a)",
                        marginBottom: 6,
                      }}
                    >
                      Component tree
                    </div>
                    <pre
                      style={{
                        fontFamily: "var(--font-mono, monospace)",
                        fontSize: 11,
                        color: "var(--text-tertiary, #6e6e7c)",
                        margin: 0,
                        whiteSpace: "pre-wrap",
                        wordBreak: "break-all",
                        lineHeight: 1.6,
                        maxHeight: 180,
                        overflow: "auto",
                        background: "rgba(0,0,0,0.2)",
                        borderRadius: 4,
                        padding: "10px 12px",
                      }}
                    >
                      {info.componentStack}
                    </pre>
                  </>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    );
  }
}
