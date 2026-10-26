/// <reference types="vitest/config" />
import { fileURLToPath } from "node:url";
import { defineConfig, type UserConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const web = fileURLToPath(new URL("../web", import.meta.url));
const here = (p: string) => fileURLToPath(new URL(p, import.meta.url));

// Vitest ships vite 5 peer types but we run vite 6, so the cast keeps tsc happy while Vitest still reads `test:`.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  // The favicon and app icons are the desktop app's own files.
  publicDir: `${web}/public`,
  resolve: {
    alias: {
      "@web": `${web}/src`,
      // web/src/index.css does `@import "tailwindcss"`. Vercel installs only cloud/node_modules, so pin it there.
      tailwindcss: here("node_modules/tailwindcss/index.css"),
    },
    // Imports in ../web/src resolve from web/node_modules, which Vercel never installs, and react would bundle twice.
    dedupe: ["react", "react-dom", "markdown-it", "dompurify"],
  },
  // build.mjs clears dist and writes config.json before Vite runs, so Vite must keep it.
  build: { outDir: "dist", emptyOutDir: false },
  test: {
    globals: true,
    exclude: ["**/node_modules/**", "**/dist/**", "e2e/**"],
  },
} as UserConfig);
