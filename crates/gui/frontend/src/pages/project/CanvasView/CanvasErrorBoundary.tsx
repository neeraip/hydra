import { ExclamationTriangleIcon } from "@heroicons/react/24/outline";
import type { ReactNode } from "react";
import { Component } from "react";

export class CanvasErrorBoundary extends Component<
  { children: ReactNode },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  render() {
    const { error } = this.state;
    if (error) {
      return (
        <div
          style={{
            position: "absolute",
            inset: 0,
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            background: "var(--bg-base)",
            gap: 12,
          }}
        >
          <ExclamationTriangleIcon
            style={{ width: 32, height: 32, color: "var(--status-warning)" }}
          />
          <span
            style={{
              fontSize: 14,
              fontWeight: 600,
              color: "var(--text-primary)",
            }}
          >
            Canvas error
          </span>
          <span
            style={{
              fontSize: 12,
              color: "var(--text-tertiary)",
              maxWidth: 320,
              textAlign: "center",
            }}
          >
            {error.message}
          </span>
          <button
            className="tool-btn"
            style={{ marginTop: 8 }}
            onClick={() => this.setState({ error: null })}
          >
            Retry
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
