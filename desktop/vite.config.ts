import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const MERIDIAN_BACKEND = process.env.MERIDIAN_BACKEND ?? "http://127.0.0.1:7878";

// Vite builds the renderer into ./dist-renderer/ so it doesn't clash with
// electron-builder's `dist/` packaging output at the same level.
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
  },
});
