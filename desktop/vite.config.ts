import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

const MERIDIAN_BACKEND = process.env.MERIDIAN_BACKEND ?? "http://127.0.0.1:7878";

// Vite builds the renderer into ./dist-renderer/ so it doesn't clash with
// electron-builder's `dist/` packaging output at the same level.
//
// Two entry points:
//   index.html         — the main Meridian app
//   page-runtime.html  — the sandboxed iframe that renders LLM-authored
//                        pages. Loaded via <iframe src="/page-runtime.html">
//                        with `sandbox="allow-scripts"` (no allow-same-origin).
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      "/api": {
        target: MERIDIAN_BACKEND,
        changeOrigin: true,
        ws: true,
      },
    },
  },
  build: {
    outDir: "dist-renderer",
    emptyOutDir: true,
    sourcemap: true,
    rollupOptions: {
      input: {
        main: path.resolve(__dirname, "index.html"),
        pageRuntime: path.resolve(__dirname, "page-runtime.html"),
      },
    },
  },
});
