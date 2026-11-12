import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

// Tauri loads the built frontend from `dist/` over its own protocol, and in
// development waits for exactly this port. Do not change one without the other
// (src-tauri/tauri.conf.json, build.devUrl).
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    outDir: "dist",
    // WKWebView on the oldest macOS this app supports (12) is Safari 15.
    target: "safari15",
    emptyOutDir: true,
  },
  test: { include: ["src/**/*.test.ts"] },
});
