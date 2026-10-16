import { defineConfig, devices } from "@playwright/test";

// Own Vite port outside the `just dev` range. `reuseExistingServer` is on locally, so sharing :5173
// could silently run the suite against another worktree's code.
const E2E_PORT = 5199;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? "list" : [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: `http://127.0.0.1:${E2E_PORT}`,
    trace: "on-first-retry",
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
  ],
  webServer: {
    // Bind 127.0.0.1 explicitly: bare `vite` binds IPv6 localhost only, which the health check cannot reach.
    command: "pnpm dev --host 127.0.0.1",
    url: `http://127.0.0.1:${E2E_PORT}`,
    env: { YOGURT_VITE_PORT: String(E2E_PORT) },
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
