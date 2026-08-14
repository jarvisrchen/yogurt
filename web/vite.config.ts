/// <reference types="vitest/config" />
import { defineConfig, type UserConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Vitest 2.1 ships vite 5 peer types; we run vite 6. Casting the config to
// `UserConfig & { test: unknown }` keeps the editor + tsc --noEmit happy while
// still letting Vitest pick the `test:` block up at runtime. The triple-slash
// reference above pulls vitest/config's TestUserConfig type into the project
// so `globals: true` enables `describe`/`it`/`expect` ambient globals.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: 5173,
    strictPort: true,
    // Pin HMR client to :5173 so it doesn't try the page-origin (:7878
    // when served via the yogurt backend proxy). Without this, Vite's
    // HMR client guesses `window.location.host`, opens WS to :7878,
    // yogurt's dev_proxy rejects it ("vite proxy: refusing websocket
    // upgrade -- use http://localhost:5173 for HMR"), Vite falls back
    // — and the user sees a noisy `WebSocket connection failed` plus
    // a "Direct websocket connection fallback" warning in the console.
    hmr: {
      host: "127.0.0.1",
      clientPort: 5173,
    },
    proxy: {
      "/api": "http://localhost:7878",
      "/ws": { target: "ws://localhost:7878", ws: true },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/vitest.setup.ts"],
    // `e2e/*.spec.ts` are Playwright specs, not vitest — keep vitest out of
    // them (its default include otherwise matches `.spec.ts`).
    exclude: ["**/node_modules/**", "**/dist/**", "e2e/**"],
  },
} as UserConfig);
