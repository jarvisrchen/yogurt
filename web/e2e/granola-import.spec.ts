import { test, expect, type WebSocketRoute } from "@playwright/test";

const SETTINGS = {
  general: {
    port: 7878,
    open_browser_on_start: true,
    audio_input_device: "",
    audio_echo_feature: false,
    audio_echo_output_device: "",
    audio_echo_enabled: false,
    audio_echo_buffer: 512,
    first_run_completed: true,
    stt_provider: "local",
    stt_model: "large-v3",
    meeting_detection: true,
    meeting_detection_focus: true,
    meeting_auto_stop: true,
  },
  providers: [
    {
      id: "p1",
      name: "Mock",
      base_url: "http://mock/v1",
      model: "m",
      is_active: true,
      created_at: 1,
      api_key_masked: "••••abcd",
      adapter: "http",
      cli_model: "",
    },
  ],
  presets: [],
  deepgram_key_masked: null,
  paths: {
    data_dir: "~/.yogurt",
    keys: "~/.yogurt/keys.json",
    notes: "~/.yogurt/notes",
  },
};

test("granola import: key, preview, progress on every page, done", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  let key: string | null = null;
  let running = false;
  let sockets: WebSocketRoute[] = [];
  const frame = (f: object) => sockets.forEach((s) => s.send(JSON.stringify(f)));

  await page.routeWebSocket(/\/ws(\/|\?|$)/, (ws) => {
    sockets.push(ws);
  });
  await page.route("**/api/session-token", (r) => r.fulfill({ json: { token: "e2e-token" } }));
  await page.route(/\/api\/settings(\?|$)/, (r) => r.fulfill({ json: SETTINGS }));
  await page.route("**/api/cloud", (r) => r.fulfill({ json: { connected: false } }));
  await page.route("**/api/audio/permission", (r) =>
    r.fulfill({ json: { screen_recording: "granted", microphone: "granted" } }),
  );
  await page.route("**/api/labels", (r) => r.fulfill({ json: [] }));
  await page.route(/\/api\/meetings(\?|$)/, (r) => r.fulfill({ json: { meetings: [], total: 0 } }));
  await page.route("**/api/import/granola/key", (r) => {
    if (r.request().method() === "PUT") {
      key = "••••wxyz";
      return r.fulfill({ json: { key } });
    }
    key = null;
    return r.fulfill({ status: 204 });
  });
  await page.route("**/api/import/granola/preview", (r) =>
    r.fulfill({ json: { total: 1365, already_imported: 0, to_import: 1365, captured_by_others: 73 } }),
  );
  await page.route(/\/api\/import\/granola$/, (r) => {
    const method = r.request().method();
    if (method === "POST") {
      running = true;
      return r.fulfill({ status: 202 });
    }
    return r.fulfill({
      json: { key, running, progress: null, last: null },
    });
  });

  await page.goto("/settings/import");
  await page.getByLabel("Granola API key").fill("grn_pasted_key");
  await page.getByRole("button", { name: "Save key" }).click();
  await expect(page.getByText("••••wxyz")).toBeVisible();
  expect(await page.evaluate(() => document.body.innerText)).not.toContain("grn_pasted_key");

  await page.getByRole("button", { name: "Preview" }).click();
  await expect(page.getByTestId("import-preview")).toHaveText(
    "1,365 meetings in Granola · 0 already imported · 73 captured by someone else",
  );
  await page.getByRole("button", { name: /Import 1,365 meetings/ }).click();
  await expect(page.getByRole("button", { name: "Cancel" })).toBeVisible();

  frame({ type: "import_progress", done: 340, total: 1365, imported: 340, skipped: 0, failed: 0 });
  await expect(page.getByText("340 of 1,365", { exact: true })).toBeVisible();
  await expect(page.getByTestId("import-pill")).toHaveText("Importing from Granola · 340 of 1,365");

  await page.getByRole("link", { name: "Library" }).click();
  await expect(page).toHaveURL(/\/$/);
  await expect(page.getByTestId("import-pill")).toBeVisible();
  frame({ type: "import_progress", done: 700, total: 1365, imported: 700, skipped: 0, failed: 0 });
  await expect(page.getByTestId("import-pill")).toHaveText("Importing from Granola · 700 of 1,365");

  await page.getByTestId("import-pill").click();
  await expect(page).toHaveURL(/\/settings\/import$/);

  frame({ type: "import_complete", imported: 1365, skipped: 0, failed: 0, failed_titles: [] });
  await expect(page.getByTestId("import-pill")).toContainText("Imported 1,365");
  await page.getByTestId("import-pill").click();
  await expect(page).toHaveURL(/\/$/);
  await expect(page.getByTestId("import-pill")).toHaveCount(0);
});
