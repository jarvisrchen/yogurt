import { test, expect, type Page } from "@playwright/test";

const MEETING_ID = "019f0000-0000-7000-8000-000000000077";

const MEETING = {
  id: MEETING_ID,
  title: "Pricing review",
  started_at: 1_700_000_000_000,
  ended_at: null,
  notes_md: "",
  enriched_md: null,
  transcript_json: "[]",
  starred: false,
  labels: [],
  created_at: "2026-08-30T00:00:00.000Z",
  updated_at: "2026-08-30T00:00:00.000Z",
};

const BULLETS = [
  "Goal: settle the Pro price before the launch email",
  "Dana proposes $18/month, matching the closest competitor",
  "Sam worries $18 undercuts the annual plan",
  "Annual stays at $180, two months free",
  "Churn data from the beta favors a lower entry price",
  "Agreed to test $18 against $22 for two weeks",
  "Sam owns the pricing page copy",
  "Dana sets up the experiment flag by Friday",
  "Revisit after the first week's conversion numbers",
].map((text, i) => ({ ts_ms: (i + 1) * 60_000, text }));

const SETTINGS = {
  general: {
    port: 7878,
    first_run_completed: true,
    stt_provider: "local",
    stt_model: "large-v3",
    live_recap: true,
    live_recap_placement: "main",
  },
  providers: [
    { id: "p1", name: "Mock", base_url: "http://mock/v1", model: "m", is_active: true, created_at: 1, api_key_masked: "••••abcd" },
  ],
  presets: [],
};

async function mockBackend(page: Page) {
  await page.routeWebSocket(/\/ws(\/|\?|$)/, (ws) => {
    ws.send(
      JSON.stringify({
        type: "live_recap",
        bullets: BULLETS,
        status: "idle",
        error: null,
        updated_at: Date.now(),
        paused: false,
      }),
    );
  });
  await page.route("**/api/session-token", (r) => r.fulfill({ json: { token: "e2e-token" } }));
  await page.route(/\/api\/settings(\?|$)/, (r) => r.fulfill({ json: SETTINGS }));
  await page.route("**/api/audio/permission", (r) =>
    r.fulfill({ json: { screen_recording: "granted", microphone: "granted" } }),
  );
  await page.route(/\/api\/meetings\/active(\?|$)/, (r) =>
    r.fulfill({ json: { id: MEETING_ID, title: MEETING.title, started_at: MEETING.started_at, stt: "local" } }),
  );
  await page.route(/\/api\/meetings\/[0-9a-f-]+(\?|$)/, (r) => r.fulfill({ json: MEETING }));
  await page.route(/\/api\/meetings(\?|$)/, (r) => r.fulfill({ json: { meetings: [MEETING], total: 1 } }));
}

test("the recap card grows from its bottom handle and keeps the height (MTG-29)", async ({ page }) => {
  await mockBackend(page);
  await page.goto(`/meeting/${MEETING_ID}`);

  const list = page.getByTestId("recap-list");
  await expect(list.getByText(BULLETS[8].text)).toBeVisible();
  const handle = page.getByRole("separator", { name: "Resize recap" });
  await expect(handle).toHaveAttribute("aria-valuenow", "148");
  const box = (await handle.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2, box.y + 200, { steps: 5 });
  await page.mouse.up();

  const grown = Number(await handle.getAttribute("aria-valuenow"));
  expect(grown).toBeGreaterThan(300);
  expect((await list.boundingBox())!.height).toBeCloseTo(grown, 0);
  await page.reload();
  await expect(page.getByTestId("recap-list").getByText(BULLETS[0].text)).toBeVisible();
  await expect(handle).toHaveAttribute("aria-valuenow", String(grown));
});
