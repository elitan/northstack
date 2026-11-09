import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { tanstackRouter } from "@tanstack/router-plugin/vite";

// In development, Vite serves the UI with hot reload and proxies API calls to
// the Rust server on :3000. In production the Rust binary serves web/dist.
export default defineConfig({
  plugins: [
    // File-based routing: src/routes/** -> src/routeTree.gen.ts (generated).
    tanstackRouter({ target: "react", autoCodeSplitting: true, quoteStyle: "double" }),
    react(),
  ],
  server: {
    port: 5173,
    proxy: {
      "/rpc": "http://localhost:3000",
      "/healthz": "http://localhost:3000",
    },
  },
  build: { outDir: "dist", emptyOutDir: true },
});
