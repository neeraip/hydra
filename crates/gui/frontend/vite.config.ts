import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
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
  },
});
