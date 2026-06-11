import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "maplibre-gl/dist/maplibre-gl.css";
import "./app.css";
import { App } from "./App";
import { AppProvider } from "./AppContext";
import { CanvasLayersProvider } from "./canvas/layers-context";
import { CanvasSelectionProvider } from "./canvas/selection-context";
import { CanvasStatusProvider } from "./canvas/status-context";
import { ErrorBoundary } from "./components/ui/ErrorBoundary";
import { NetworkVersionProvider } from "./hooks/NetworkVersionContext";

// Apply persisted accessibility settings before React renders so the DOM
// attribute is in place before first paint.
const root = document.documentElement;
if (localStorage.getItem("hydra2-reduced-motion") === "true")
  root.setAttribute("data-reduced-motion", "true");
if (localStorage.getItem("hydra2-high-contrast") === "true")
  root.setAttribute("data-high-contrast", "true");

// In the packaged Tauri app, suppress the native WebView context menu
// (which exposes Reload / Inspect Element). Individual controls can opt out
// by adding data-allow-native-context-menu="true".
function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

if (isTauri()) {
  document.addEventListener(
    "contextmenu",
    (e) => {
      const target = e.target as Element | null;
      if (target?.closest("[data-allow-native-context-menu='true']")) return;
      e.preventDefault();
    },
    true,
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary scope="Application">
      <NetworkVersionProvider>
        <CanvasLayersProvider>
          <CanvasSelectionProvider>
            <CanvasStatusProvider>
              <AppProvider>
                <App />
              </AppProvider>
            </CanvasStatusProvider>
          </CanvasSelectionProvider>
        </CanvasLayersProvider>
      </NetworkVersionProvider>
    </ErrorBoundary>
  </StrictMode>,
);
