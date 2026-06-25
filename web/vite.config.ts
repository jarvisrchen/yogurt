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
    proxy: {
      "/api": "http://localhost:7878",
      "/ws": { target: "ws://localhost:7878", ws: true },
    },
  },
  test: {
    environment: "jsdom",
    globals: true,
  },
} as UserConfig);
