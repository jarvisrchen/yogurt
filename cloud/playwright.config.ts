import { defineConfig, devices } from "@playwright/test";

// The local stack from cloud/supabase (`just test-cloud-web`). The publishable key is the Supabase CLI's fixed local one.
const API = process.env.YOGURT_CLOUD_E2E_API ?? "http://127.0.0.1:56431";
const KEY = process.env.YOGURT_CLOUD_E2E_KEY ?? "sb_publishable_ACJWlzQHlZjBrEguHvfOxg_3BJgxAaH";
const PORT = 5191;

const phone = { ...devices["Pixel 7"], viewport: { width: 390, height: 844 } };
const desktop = { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } };
const project = (name: string, use: object, colorScheme: "light" | "dark") => ({ name, use: { ...use, colorScheme } });

export default defineConfig({
  testDir: "./e2e",
  // One Supabase project has one owner, so the specs share state and run one at a time.
  workers: 1,
  fullyParallel: false,
  reporter: [["list"]],
  timeout: 90_000,
  use: { baseURL: `http://127.0.0.1:${PORT}`, trace: "retain-on-failure" },
  projects: [
    project("phone-light", phone, "light"),
    project("phone-dark", phone, "dark"),
    project("desktop-light", desktop, "light"),
    project("desktop-dark", desktop, "dark"),
  ],
  webServer: {
    command: `pnpm build && pnpm exec vite preview --host 127.0.0.1 --port ${PORT} --strictPort`,
    url: `http://127.0.0.1:${PORT}`,
    // The LLM is e2e/chat.spec.ts's fake provider.
    env: { SUPABASE_URL: API, SUPABASE_PUBLISHABLE_KEY: KEY, YOGURT_LLM_BASE_URL: "http://127.0.0.1:5192/v1", YOGURT_LLM_MODEL: "fake-model", YOGURT_LLM_API_KEY: "fake-key" },
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
