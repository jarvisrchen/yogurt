import { defineConfig, devices } from "@playwright/test";

/**
 * Playwright E2E smoke config.
 *
 * These specs drive the REAL React app (routing + rendering) against a
 * browser-level mock of the Rust backend (`page.route`), so they run in CI
 * with NO keychain, NO Deepgram/Minimax keys, and NO live LLM — the exact
 * things a real backend needs and a CI box can't have. They exist to catch
 * integration regressions that the vitest unit tests mock away, e.g. a
 * Library card wired to the wrong route.
 *
 * Run: `pnpm e2e` (or `just test-e2e`). The config starts Vite itself.
 */
export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? "list" : [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: "http://127.0.0.1:5173",
    trace: "on-first-retry",
  },
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
  ],
  webServer: {
    // Bind 127.0.0.1 explicitly — bare `vite` binds IPv6 `localhost` only,
    // which the 127.0.0.1 health check below can't reach (matches
    // scripts/run-frontend.sh).
    command: "pnpm dev --host 127.0.0.1",
    url: "http://127.0.0.1:5173",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
});
