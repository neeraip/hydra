import {
  ArrowTopRightOnSquareIcon,
  EyeIcon,
  EyeSlashIcon,
  XMarkIcon,
} from "@heroicons/react/16/solid";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useState } from "react";
import { useAppState } from "../../AppContext";
import {
  type BasemapVisibility,
  basemapIdForCatalogStyle,
  isBasemapStyleHidden,
} from "../../canvas/Basemap";
import {
  type BasemapProvider,
  connectBasemapProvider,
  disconnectBasemapProvider,
  refreshBasemapProviders,
  setBasemapStylesHidden,
  useBasemapProviders,
  useBasemapVisibility,
} from "../../hooks/basemapProviders";
import { formatIpcError } from "../../hooks/ipc";
import { ModalBackdrop, stopBackdropEvents } from "../ui/ModalBackdrop";

/** Small pill badge (Free/Paid, Connected). */
function Badge({ text, accent = false }: { text: string; accent?: boolean }) {
  return (
    <span
      style={{
        fontSize: "var(--text-xs)",
        fontWeight: 700,
        letterSpacing: "0.05em",
        textTransform: "uppercase",
        padding: "2px 6px",
        borderRadius: 4,
        background: accent ? "var(--accent-dim)" : "var(--bg-input)",
        color: accent ? "var(--accent)" : "var(--text-tertiary)",
        border: "1px solid var(--border)",
        fontFamily: "var(--font-ui)",
        whiteSpace: "nowrap",
      }}
    >
      {text}
    </span>
  );
}

/** Eye toggle for one style (or a whole provider). Visible = eye open. */
function VisibilityToggle({
  hidden,
  label,
  onToggle,
}: {
  hidden: boolean;
  label: string;
  onToggle: () => void;
}) {
  const Icon = hidden ? EyeSlashIcon : EyeIcon;
  return (
    <button
      type="button"
      className={`tool-btn${hidden ? "" : " active"}`}
      onClick={onToggle}
      data-tooltip={label}
      data-tooltip-pos="bottom"
      aria-label={label}
      style={{
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        flexShrink: 0,
      }}
    >
      <Icon style={{ width: 14, height: 14 }} />
    </button>
  );
}

/** One provider card: identity, connection management, and style rows. */
function ProviderCard({
  provider,
  visibility,
  showToast,
}: {
  provider: BasemapProvider;
  visibility: BasemapVisibility;
  showToast: (
    message: string,
    type?: "info" | "success" | "warn" | "error",
  ) => void;
}) {
  const [tokenDraft, setTokenDraft] = useState("");
  const [replacing, setReplacing] = useState(false);
  const [connecting, setConnecting] = useState(false);
  const [disconnecting, setDisconnecting] = useState(false);

  const isPaid = provider.kind === "paid";
  // Free providers (built-in or not) never need a credential — the backend
  // reports them permanently connected.
  const showTokenForm = isPaid && (!provider.connected || replacing);

  const styleRows = provider.styles.map((s) => ({
    style: s,
    basemapId: basemapIdForCatalogStyle(provider.id, s.id),
  }));
  // Provider styles are hidden by default (OpenFreeMap's are visible) — the
  // eyes flip explicit overrides via setBasemapStylesHidden.
  const allHidden =
    styleRows.length > 0 &&
    styleRows.every((r) => isBasemapStyleHidden(r.basemapId, visibility));

  const handleConnect = useCallback(async () => {
    setConnecting(true);
    try {
      // Live token validation on the backend — slow, hence the busy state.
      await connectBasemapProvider(provider.id, tokenDraft);
      setTokenDraft("");
      setReplacing(false);
      showToast(`${provider.displayName} connected.`, "success");
    } catch (err) {
      showToast(formatIpcError(err), "error");
    } finally {
      setConnecting(false);
    }
  }, [provider.id, provider.displayName, tokenDraft, showToast]);

  const handleDisconnect = useCallback(async () => {
    setDisconnecting(true);
    try {
      await disconnectBasemapProvider(provider.id);
      setReplacing(false);
      setTokenDraft("");
      showToast(`${provider.displayName} disconnected.`, "info");
    } catch (err) {
      showToast(formatIpcError(err), "error");
    } finally {
      setDisconnecting(false);
    }
  }, [provider.id, provider.displayName, showToast]);

  return (
    <div
      style={{
        border: "1px solid var(--border)",
        borderRadius: 8,
        background: "var(--bg-card)",
        padding: 12,
        display: "flex",
        flexDirection: "column",
        gap: 8,
      }}
    >
      {/* Header: name + badges + provider-wide eye */}
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span
          style={{
            fontSize: "var(--text-lg)",
            fontWeight: 600,
            color: "var(--text-primary)",
            fontFamily: "var(--font-ui)",
          }}
        >
          {provider.displayName}
        </span>
        <Badge text={isPaid ? "Paid" : "Free"} />
        {provider.connected ? (
          <Badge text="Connected" accent />
        ) : (
          <Badge text="Not connected" />
        )}
        <div style={{ flex: 1 }} />
        <VisibilityToggle
          hidden={allHidden}
          label={
            allHidden
              ? "Show all styles in the basemap picker"
              : "Hide all styles from the basemap picker"
          }
          onToggle={() =>
            setBasemapStylesHidden(
              styleRows.map((r) => r.basemapId),
              !allHidden,
            )
          }
        />
      </div>

      <div
        style={{
          fontSize: "var(--text-sm)",
          color: "var(--text-tertiary)",
          fontFamily: "var(--font-ui)",
          lineHeight: 1.4,
        }}
      >
        {provider.attribution}
      </div>

      {/* Paid credential management */}
      {isPaid && provider.connected && !replacing && (
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span
            style={{
              fontSize: "var(--text-md)",
              color: "var(--text-secondary)",
              fontFamily: "var(--font-mono)",
            }}
          >
            {provider.tokenPreview ?? "…"}
          </span>
          <div style={{ flex: 1 }} />
          <button
            type="button"
            className="tool-btn"
            onClick={() => setReplacing(true)}
            style={{
              width: "auto",
              height: 24,
              padding: "0 8px",
              fontSize: "var(--text-sm)",
            }}
          >
            Replace
          </button>
          <button
            type="button"
            className="tool-btn"
            disabled={disconnecting}
            onClick={() => void handleDisconnect()}
            style={{
              width: "auto",
              height: 24,
              padding: "0 8px",
              fontSize: "var(--text-sm)",
              color: "var(--status-error)",
            }}
          >
            {disconnecting ? "Disconnecting…" : "Disconnect"}
          </button>
        </div>
      )}
      {showTokenForm && (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            gap: 6,
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <label
              htmlFor={`basemap-token-${provider.id}`}
              style={{
                fontSize: "var(--text-sm)",
                color: "var(--text-secondary)",
                fontFamily: "var(--font-ui)",
                whiteSpace: "nowrap",
              }}
            >
              {provider.tokenLabel ?? "Token"}
            </label>
            <div style={{ flex: 1 }} />
            <button
              type="button"
              onClick={() => void openUrl(provider.signupUrl)}
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 4,
                border: "none",
                background: "transparent",
                color: "var(--accent)",
                cursor: "pointer",
                fontSize: "var(--text-sm)",
                fontFamily: "var(--font-ui)",
                padding: 0,
              }}
            >
              Get a {provider.tokenLabel?.toLowerCase() ?? "token"}
              <ArrowTopRightOnSquareIcon style={{ width: 11, height: 11 }} />
            </button>
          </div>
          <div style={{ display: "flex", gap: 8 }}>
            <input
              id={`basemap-token-${provider.id}`}
              type="password"
              value={tokenDraft}
              disabled={connecting}
              onChange={(e) => setTokenDraft(e.currentTarget.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" && tokenDraft.trim() && !connecting) {
                  void handleConnect();
                }
              }}
              placeholder={`Paste your ${provider.tokenLabel?.toLowerCase() ?? "token"}`}
              style={{
                flex: 1,
                fontSize: "var(--text-md)",
                padding: "6px 10px",
                background: "var(--bg-input, var(--bg-card))",
                border: "1px solid var(--border)",
                borderRadius: 6,
                color: "var(--text-primary)",
                fontFamily: "var(--font-mono)",
              }}
            />
            <button
              type="button"
              className="tool-btn"
              disabled={connecting || tokenDraft.trim() === ""}
              onClick={() => void handleConnect()}
              style={{
                width: "auto",
                height: 28,
                padding: "0 10px",
                fontSize: "var(--text-md)",
              }}
            >
              {connecting ? "Validating…" : "Connect"}
            </button>
            {replacing && (
              <button
                type="button"
                className="tool-btn"
                disabled={connecting}
                onClick={() => {
                  setReplacing(false);
                  setTokenDraft("");
                }}
                style={{
                  width: "auto",
                  height: 28,
                  padding: "0 10px",
                  fontSize: "var(--text-md)",
                }}
              >
                Cancel
              </button>
            )}
          </div>
        </div>
      )}

      {/* Style rows with per-style visibility eyes */}
      <div
        style={{
          border: "1px solid var(--border)",
          borderRadius: 6,
          overflow: "hidden",
        }}
      >
        {styleRows.map(({ style, basemapId }, idx) => {
          const hidden = isBasemapStyleHidden(basemapId, visibility);
          return (
            <div
              key={style.id}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 8,
                padding: "4px 8px",
                borderTop: idx === 0 ? "none" : "1px solid var(--border)",
              }}
            >
              <span
                style={{
                  fontSize: "var(--text-md)",
                  color: hidden
                    ? "var(--text-tertiary)"
                    : "var(--text-primary)",
                  fontFamily: "var(--font-ui)",
                  flex: 1,
                  minWidth: 0,
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                {style.displayName}
                {hidden && (
                  <span style={{ color: "var(--text-tertiary)" }}>
                    {" "}
                    (hidden)
                  </span>
                )}
              </span>
              <VisibilityToggle
                hidden={hidden}
                label={
                  hidden
                    ? "Show in the basemap picker"
                    : "Hide from the basemap picker"
                }
                onToggle={() => setBasemapStylesHidden([basemapId], !hidden)}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}

/**
 * Basemap-providers management modal: lists the curated catalog with
 * connection state, paid-token connect/replace/disconnect, and per-style /
 * per-provider visibility eyes feeding the (per-machine) hidden-styles pref.
 * Opened from the canvas basemap picker ("Manage basemaps…") and from the
 * Settings page.
 */
export function BasemapProvidersModal() {
  const { basemapProvidersModalOpen, closeBasemapProvidersModal, showToast } =
    useAppState();
  const providers = useBasemapProviders();
  const visibility = useBasemapVisibility();

  // Re-fetch connection status whenever the modal opens (tokens may have
  // been changed in another window / earlier session).
  useEffect(() => {
    if (basemapProvidersModalOpen) void refreshBasemapProviders();
  }, [basemapProvidersModalOpen]);

  useEffect(() => {
    if (!basemapProvidersModalOpen) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") closeBasemapProvidersModal();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [basemapProvidersModalOpen, closeBasemapProvidersModal]);

  if (!basemapProvidersModalOpen) return null;

  return (
    <ModalBackdrop onDismiss={closeBasemapProvidersModal} zIndex={205}>
      <div
        {...stopBackdropEvents}
        style={{
          width: "min(720px, 92vw)",
          maxHeight: "min(640px, 86vh)",
          background: "var(--bg-card)",
          border: "1px solid var(--border)",
          borderRadius: 10,
          backdropFilter: "blur(24px)",
          display: "flex",
          flexDirection: "column",
          overflow: "hidden",
          boxShadow: "0 24px 80px rgba(0,0,0,0.5)",
        }}
      >
        <div
          style={{
            flexShrink: 0,
            minHeight: 52,
            borderBottom: "1px solid var(--border)",
            background: "var(--bg-panel)",
            display: "flex",
            alignItems: "center",
            padding: "0 16px",
            gap: 10,
          }}
        >
          <span
            style={{
              fontSize: "var(--text-xl)",
              fontWeight: 600,
              color: "var(--text-primary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            Basemap providers
          </span>
          <span
            style={{
              fontSize: "var(--text-md)",
              color: "var(--text-tertiary)",
              fontFamily: "var(--font-ui)",
            }}
          >
            Connect providers and choose which styles appear in the picker
          </span>
          <div style={{ flex: 1 }} />
          <button
            type="button"
            onClick={closeBasemapProvidersModal}
            aria-label="Close"
            style={{
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              width: 28,
              height: 28,
              border: "none",
              background: "transparent",
              color: "var(--text-secondary)",
              borderRadius: 5,
              cursor: "pointer",
              padding: 0,
            }}
          >
            <XMarkIcon style={{ width: 14, height: 14 }} />
          </button>
        </div>

        <div
          style={{
            overflowY: "auto",
            display: "flex",
            flexDirection: "column",
            gap: 10,
            padding: 12,
          }}
        >
          {providers.length === 0 && (
            <div
              style={{
                padding: "16px 12px",
                fontSize: "var(--text-lg)",
                color: "var(--text-tertiary)",
                fontFamily: "var(--font-ui)",
              }}
            >
              Provider catalog unavailable.
            </div>
          )}
          {providers.map((p) => (
            <ProviderCard
              key={p.id}
              provider={p}
              visibility={visibility}
              showToast={showToast}
            />
          ))}
        </div>
      </div>
    </ModalBackdrop>
  );
}
