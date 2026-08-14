import { test, expect, type Page } from "@playwright/test";

/**
 * Smoke: opening a meeting from the Library must land on the post-meeting
 * READ view and show the saved (enriched) notes — NOT the empty live-capture
 * placeholder.
 *
 * This is the regression net for the 2026-08-13 finding where MeetingCard
 * linked to `/meeting/:id` (live capture, placeholder-only) instead of
 * `/meeting/:id/post` (hydrates enriched_md via GET /api/meetings/:id).
 * The whole backend is mocked at the browser layer so the test needs no
 * keychain, API keys, or live LLM.
 */

const MEETING = {
  id: "019f0000-0000-7000-8000-000000000001",
  title: "Quarterly planning",
  started_at: 1_700_000_000_000,
  ended_at: 1_700_003_600_000,
  notes_md: "- budget",
  // Enriched note the read view must render. The distinctive sentence below
  // only exists in enriched_md — if the app shows the live placeholder
  // instead of hydrating, this text is absent.
  enriched_md:
    '## Budget\n\n- budget\n<span data-ai-grey="" data-ts="5">The team agreed the Q3 budget lands at 1.2 million <span data-transcript-link="" data-ts="5">↳ 00:05</span></span>\n',
  transcript_json: "[]",
  starred: false,
  created_at: "2026-08-13T00:00:00.000Z",
  updated_at: "2026-08-13T00:00:00.000Z",
};

const SETTINGS = {
  general: {
    port: 7878,
    open_browser_on_start: true,
    audio_input_device: "",
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

/** Mock every backend call the SPA makes on the library → meeting path. */
async function mockBackend(page: Page) {
  await page.route("**/api/session-token", (r) =>
    r.fulfill({ json: { token: "e2e-token" } }),
  );
  await page.route(/\/api\/settings(\?|$)/, (r) => r.fulfill({ json: SETTINGS }));
  await page.route("**/api/audio/permission", (r) =>
    r.fulfill({ json: { screen_recording: "granted", microphone: "granted" } }),
  );
  // Single-meeting GET (has an id segment) — register before the list route.
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

  // Library renders the meeting card.
  const card = page.getByText("Quarterly planning");
  await expect(card).toBeVisible();

  await card.click();

  // #1 regression: must route to the post-meeting READ view.
  await expect(page).toHaveURL(/\/meeting\/[0-9a-f-]+\/post$/);

  // The read view hydrated the enriched notes …
  await expect(
    page.getByText(/Q3 budget lands at 1\.2 million/),
  ).toBeVisible();

  // … and did NOT fall back to the live-capture placeholder.
  await expect(
    page.getByText("Take sparse notes during the meeting"),
  ).toHaveCount(0);
});
