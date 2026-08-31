/// <reference types="vitest/config" />
import { defineConfig, type UserConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Vitest 2.1 ships vite 5 peer types; we run vite 6. Casting the config to
// `UserConfig & { test: unknown }` keeps the editor + tsc --noEmit happy while
// still letting Vitest pick the `test:` block up at runtime. The triple-slash
// reference above pulls vitest/config's TestUserConfig type into the project
// so `globals: true` enables `describe`/`it`/`expect` ambient globals.
// Dev-server port. `just dev` resolves a free (vite, backend) pair and
// passes the vite half down as YOGURT_VITE_PORT, so two worktrees can run
// side by side; the backend proxy is pointed at the same number via
// YOGURT_VITE_BASE. 5173 stays the default for a bare `pnpm dev`.
const PORT = Number(process.env.YOGURT_VITE_PORT ?? 5173);
// Where /api and /ws go when the page is opened on Vite directly. Follows
// the backend the same way, so a worktree pair stays self-consistent.
const BACKEND = Number(process.env.YOGURT_BACKEND_PORT ?? 7878);

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: PORT,
    strictPort: true,
    // Pin the HMR client to Vite's own port so it doesn't try the
    // page-origin (the backend's port, when served via the yogurt proxy).
    // Without this, Vite's HMR client guesses `window.location.host`,
    // yogurt's dev_proxy rejects it ("vite proxy: refusing websocket
    // upgrade -- use the Vite URL for HMR"), Vite falls back
    // — and the user sees a noisy `WebSocket connection failed` plus
    // a "Direct websocket connection fallback" warning in the console.
    hmr: {
      host: "127.0.0.1",
      clientPort: PORT,
    },
    proxy: {
      "/api": `http://localhost:${BACKEND}`,
      "/ws": { target: `ws://localhost:${BACKEND}`, ws: true },
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
