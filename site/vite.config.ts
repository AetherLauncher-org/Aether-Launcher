import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

/**
 * The project website: a small static React + Fluent UI v9 app, built with the repository's
 * own dependencies. Run from the repository root:
 *
 *   npm run site:dev     # http://localhost:5190
 *   npm run site:build   # writes site/dist, ready for Cloudflare Pages
 */
export default defineConfig({
  root: "site",
  plugins: [react()],
  server: { port: 5190, strictPort: true },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2020",
    // Fluent is big; split it from the app so a content change does not bust its cache.
    rollupOptions: {
      output: {
        manualChunks: (id: string) => (id.includes("@fluentui") ? "fluent" : undefined),
      },
    },
  },
});
