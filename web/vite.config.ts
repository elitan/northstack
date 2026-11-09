import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { tanstackRouter } from "@tanstack/router-plugin/vite";

// In development, Vite serves the UI with hot reload and proxies API calls to
// the Rust server on :3000. In production the Rust binary serves web/dist.
// The Rust server's port; `just dev` exports PORT from .env. (Declared here
// so the config doesn't need @types/node.)
declare const process: { env: Record<string, string | undefined> };
const api = `http://localhost:${process.env.PORT ?? 3000}`;

export default defineConfig({
  plugins: [
    // File-based routing: src/routes/** -> src/routeTree.gen.ts (generated).
    tanstackRouter({ target: "react", autoCodeSplitting: true, quoteStyle: "double" }),
    react(),
  ],
  server: {
    strictPort: true,
    port: 5173,
    proxy: {
      "/rpc": api,
      "/healthz": api,
    },
  },
  build: { outDir: "dist", emptyOutDir: true },
});
