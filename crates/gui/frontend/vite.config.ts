import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    // A literal address, not the default `localhost`. Node resolves names
    // verbatim since v17, so binding "localhost" can produce an IPv6-only
    // listener on `[::1]` depending on DNS and /etc/hosts. Tauri's webview
    // then resolves its devUrl to 127.0.0.1, finds nothing listening, and
    // opens a window that never loads the app — a blank window rather than an
    // error, so it reads as "the GUI didn't start". Whether it happened
    // depended on the machine's resolver state, which made it intermittent.
    // `tauri.conf.json`'s devUrl uses the same literal address; both must
    // agree, and neither should name a host.
    host: "127.0.0.1",
    port: 5174,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  // Vite env prefixes are literal startsWith strings (no globs): this
  // exposes TAURI_ENV_PLATFORM etc. to client code via import.meta.env.
  envPrefix: ["VITE_", "TAURI_ENV_"],
  optimizeDeps: {
    // maplibre-gl ships a web-worker entry that Vite's dependency pre-bundler
    // mishandles: it emits a reference to `.vite/deps/maplibre-gl-worker.mjs`
    // without producing the file, so the basemap dies with a dev-only 404.
    // Excluding it serves maplibre-gl's ESM as-is and sidesteps the worker
    // breakage. Dev-only — the production Rollup build never uses this path.
    exclude: ["maplibre-gl"],
  },
  build: {
    target: ["chrome120", "safari16"],
    minify: !process.env.TAURI_ENV_DEBUG ? "esbuild" : false,
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
    // CanvasView bundles maplibre-gl + deck.gl which together exceed the
    // default 500 kB threshold. This is expected — both are monolithic
    // third-party mapping libraries. The chunk is lazy-loaded so it does
    // not affect initial page load.
    chunkSizeWarningLimit: 2000,
  },
  test: {
    // Run in a Node environment — no DOM needed for pure logic tests.
    // Tests that need the browser environment can opt in with a
    // `@vitest-environment jsdom` docblock comment (after installing jsdom).
    environment: "node",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    // Applies to every test; the DOM-only parts no-op under `node`.
    setupFiles: ["./src/test-setup.ts"],
  },
});
