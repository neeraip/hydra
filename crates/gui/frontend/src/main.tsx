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
import { NetworkDataProvider } from "./hooks/NetworkDataContext";
import { NetworkVersionProvider } from "./hooks/NetworkVersionContext";

// Apply persisted accessibility settings before React renders so the DOM
// attribute is in place before first paint.
const root = document.documentElement;
if (localStorage.getItem("hydra2-reduced-motion") === "true")
  root.setAttribute("data-reduced-motion", "true");
if (localStorage.getItem("hydra2-high-contrast") === "true")
  root.setAttribute("data-high-contrast", "true");

// biome-ignore lint/style/noNonNullAssertion: React mounts into the static root element provided by index.html.
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ErrorBoundary scope="Application">
      <NetworkVersionProvider>
        <NetworkDataProvider>
          <CanvasLayersProvider>
            <CanvasSelectionProvider>
              <CanvasStatusProvider>
                <AppProvider>
                  <App />
                </AppProvider>
              </CanvasStatusProvider>
            </CanvasSelectionProvider>
          </CanvasLayersProvider>
        </NetworkDataProvider>
      </NetworkVersionProvider>
    </ErrorBoundary>
  </StrictMode>,
);
