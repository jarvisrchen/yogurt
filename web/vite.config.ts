/// <reference types="vitest/config" />
import { defineConfig, type UserConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Dev-server port. `just dev` resolves a free (vite, backend) pair and
// passes the vite half down as YOGURT_VITE_PORT, so two worktrees can run
// side by side; the backend proxy is pointed at the same number via
// YOGURT_VITE_BASE. 5173 stays the default for a bare `pnpm dev`.
const PORT = Number(process.env.YOGURT_VITE_PORT ?? 5173);
// Where /api and /ws go when the page is opened on Vite directly. Follows
// the backend the same way, so a worktree pair stays self-consistent.
const BACKEND = Number(process.env.YOGURT_BACKEND_PORT ?? 7878);

// Vitest ships vite 5 peer types but we run vite 6, so the cast keeps tsc happy while Vitest still reads `test:`.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    port: PORT,
    strictPort: true,
    // Pin the HMR client to Vite's own port. Otherwise it targets the page origin (the backend port behind
    // the yogurt proxy), the proxy refuses the websocket upgrade, and the console fills with connection warnings.
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
    // `e2e/*.spec.ts` are Playwright specs; vitest's default include would otherwise match them.
    exclude: ["**/node_modules/**", "**/dist/**", "e2e/**"],
  },
} as UserConfig);
