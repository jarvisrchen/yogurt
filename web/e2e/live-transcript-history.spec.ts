import { test, expect, type Page } from "@playwright/test";

/**
 * A live meeting's transcript dock must keep its persisted history across a
 * client-side navigation. The second visit renders with a warm React Query
 * cache (so `transcript_json` seeds the dock immediately) while
 * `/api/session-token` is still unresolved. The connect effect once cleared
 * `events` on that null token. A hard refresh hides the bug because the cold
 * cache seeds after the token.
 */

const MEETING_ID = "019f0000-0000-7000-8000-000000000042";

/** Two persisted lines. Neither is ever re-sent over the WS in this test. */
const TRANSCRIPT = [
  { ts_ms: 1_000, channel: "me", text: "spoken before the navigation" },
  { ts_ms: 4_000, channel: "them", text: "and the reply that followed" },
];

const MEETING = {
  id: MEETING_ID,
  title: "Standup in progress",
  started_at: 1_700_000_000_000,
  ended_at: null,
  notes_md: "",
  enriched_md: null,
  transcript_json: JSON.stringify(TRANSCRIPT),
  starred: false,
  labels: [],
  created_at: "2026-08-30T00:00:00.000Z",
  updated_at: "2026-08-30T00:00:00.000Z",
};

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
    },
  ],
  presets: [],
};

async function mockBackend(page: Page) {
  // No backend: answer every socket in the browser so nothing reaches Vite's proxy.
  await page.routeWebSocket(/\/ws(\/|\?|$)/, () => {});

  await page.route("**/api/session-token", (r) =>
    r.fulfill({ json: { token: "e2e-token" } }),
  );
  await page.route(/\/api\/settings(\?|$)/, (r) => r.fulfill({ json: SETTINGS }));
  await page.route("**/api/audio/permission", (r) =>
    r.fulfill({ json: { screen_recording: "granted", microphone: "granted" } }),
  );
  // The recording meeting's card links to the live route, so Meeting.tsx skips POST /start.
  await page.route(/\/api\/meetings\/active(\?|$)/, (r) =>
    r.fulfill({
      json: { id: MEETING_ID, title: MEETING.title, started_at: MEETING.started_at, stt: "local" },
    }),
  );
  await page.route(/\/api\/meetings\/[0-9a-f-]+(\?|$)/, (r) =>
    r.fulfill({ json: MEETING }),
  );
  await page.route(/\/api\/meetings(\?|$)/, (r) => r.fulfill({ json: { meetings: [MEETING], total: 1 } }));
}

test("live transcript history survives a Library round trip (MTG-1)", async ({
  page,
}) => {
  await mockBackend(page);

  await page.goto("/");
  await page.getByText("Standup in progress").first().click();
  await expect(page).toHaveURL(new RegExp(`/meeting/${MEETING_ID}$`));

  const openDock = page.getByRole("button", { name: "Show live transcript" });
  await openDock.click();
  const panel = page.getByTestId("transcript-dock-panel");
  await expect(panel.getByText("spoken before the navigation")).toBeVisible();
  await expect(panel.getByText("and the reply that followed")).toBeVisible();

  // Navigate client-side via the Library link: a reload repopulates the dock either way.
  await page.getByRole("link", { name: /Back to library/ }).click();
  await expect(page).toHaveURL(/\/$/);
  await page.getByText("Standup in progress").first().click();
  await expect(page).toHaveURL(new RegExp(`/meeting/${MEETING_ID}$`));

  await page.getByRole("button", { name: "Show live transcript" }).click();
  await expect(
    page.getByTestId("transcript-dock-panel").getByText("spoken before the navigation"),
  ).toBeVisible();
  await expect(
    page.getByTestId("transcript-dock-panel").getByText("and the reply that followed"),
  ).toBeVisible();
});
