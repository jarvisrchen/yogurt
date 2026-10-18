import { test, expect, type Page } from "@playwright/test";

const MEETING = {
  id: "019f0000-0000-7000-8000-000000000001",
  title: "Quarterly planning",
  started_at: 1_700_000_000_000,
  ended_at: 1_700_003_600_000,
  notes_md: "- budget",
  // Only in enriched_md: absent if the app shows the live placeholder.
  enriched_md:
    '## Budget\n\n- budget\n<span data-ai-grey="" data-ts="5">The team agreed the Q3 budget lands at 1.2 million <span data-transcript-link="" data-ts="5">↳ 00:05</span></span>\n',
  transcript_json: "[]",
  starred: false,
  // Required by the wire type; MeetingCard reads `labels.length`.
  labels: [],
  created_at: "2026-08-13T00:00:00.000Z",
  updated_at: "2026-08-13T00:00:00.000Z",
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
  await page.route("**/api/session-token", (r) =>
    r.fulfill({ json: { token: "e2e-token" } }),
  );
  await page.route(/\/api\/settings(\?|$)/, (r) => r.fulfill({ json: SETTINGS }));
  await page.route("**/api/audio/permission", (r) =>
    r.fulfill({ json: { screen_recording: "granted", microphone: "granted" } }),
  );
  // Register before the list route.
  await page.route(/\/api\/meetings\/[0-9a-f-]+(\?|$)/, (r) =>
    r.fulfill({ json: MEETING }),
  );
  await page.route(/\/api\/meetings(\?|$)/, (r) =>
    r.fulfill({ json: [MEETING] }),
  );
}

test("opening a library meeting shows its saved notes, not the live placeholder", async ({
  page,
}) => {
  await mockBackend(page);

  await page.goto("/");

  const card = page.getByText("Quarterly planning");
  await expect(card).toBeVisible();

  await card.click();

  await expect(page).toHaveURL(/\/meeting\/[0-9a-f-]+\/post$/);

  await expect(
    page.getByText(/Q3 budget lands at 1\.2 million/),
  ).toBeVisible();

  await expect(
    page.getByText("Take sparse notes during the meeting"),
  ).toHaveCount(0);
});
