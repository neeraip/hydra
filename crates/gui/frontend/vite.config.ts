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
  envPrefix: ["VITE_", "TAURI_ENV_*"],
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
