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
  has_enriched: true,
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
  // The Shell's app-wide socket would otherwise hit Vite's proxy with no backend behind it.
  await page.routeWebSocket(/\/ws(\/|\?|$)/, () => {});
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
    r.fulfill({ json: { meetings: [MEETING], total: 1 } }),
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

test("a meeting's Library link and Back both return to the label, search, and scroll it was opened from", async ({
  page,
}) => {
  await mockBackend(page);
  await page.route("**/api/labels", (r) =>
    r.fulfill({
      json: [{ id: "L1", name: "Build", color: "blue", meeting_count: 25, archived_count: 0, position: 0, updated_at: 1 }],
    }),
  );
  const many = Array.from({ length: 25 }, (_, i) => ({
    ...MEETING,
    id: `019f0000-0000-7000-8000-0000000000${String(i).padStart(2, "0")}`,
    title: `Framing ${i}`,
  }));
  await page.route(/\/api\/meetings(\/search)?(\?|$)/, (r) =>
    r.fulfill({ json: { meetings: many, total: 25 } }),
  );
  await page.setViewportSize({ width: 1280, height: 600 });

  await page.goto("/label/L1");
  await page.getByLabel("Search notes and transcripts").fill("framing");
  await expect(page).toHaveURL(/\/label\/L1\?q=framing$/);
  await page.getByText("Framing 20", { exact: true }).scrollIntoViewIfNeeded();
  const y = await page.evaluate(() => scrollY);
  expect(y).toBeGreaterThan(300);

  await page.getByText("Framing 20", { exact: true }).click();
  await expect(page).toHaveURL(/\/post$/);
  await expect.poll(() => page.evaluate(() => scrollY)).toBe(0);
  await page.getByRole("link", { name: "Library" }).click();
  await expect(page).toHaveURL(/\/label\/L1\?q=framing$/);
  await expect(page.getByLabel("Search notes and transcripts")).toHaveValue("framing");
  await expect.poll(() => page.evaluate(() => scrollY)).toBe(y);

  await page.getByText("Framing 20", { exact: true }).click();
  await expect(page).toHaveURL(/\/post$/);
  await page.goBack();
  await expect(page).toHaveURL(/\/label\/L1\?q=framing$/);
  await expect(page.getByLabel("Search notes and transcripts")).toHaveValue("framing");
  await expect.poll(() => page.evaluate(() => scrollY)).toBe(y);

  // Another view keeps the search and opens at its top.
  await page.getByRole("link", { name: "All meetings" }).click();
  await expect(page).toHaveURL(/\/\?q=framing$/);
  await expect(page.getByLabel("Search notes and transcripts")).toHaveValue("framing");
  await expect.poll(() => page.evaluate(() => scrollY)).toBe(0);
});

test("a page change, and a Library link with no remembered view, open at the top", async ({ page }) => {
  await mockBackend(page);
  await page.route("**/api/labels", (r) => r.fulfill({ json: [] }));
  const many = Array.from({ length: 25 }, (_, i) => ({
    ...MEETING,
    id: `019f0000-0000-7000-8000-0000000000${String(i).padStart(2, "0")}`,
    title: `Standup ${i}`,
  }));
  await page.route(/\/api\/meetings(\?|$)/, (r) => {
    const u = new URL(r.request().url());
    const offset = Number(u.searchParams.get("offset") ?? 0);
    const limit = Number(u.searchParams.get("limit") ?? 25);
    r.fulfill({ json: { meetings: many.slice(offset, offset + limit), total: 25 } });
  });
  await page.setViewportSize({ width: 1280, height: 400 });
  const bottom = () => page.evaluate(() => scrollTo(0, document.body.scrollHeight));
  const y = () => page.evaluate(() => scrollY);

  await page.goto("/?size=10");
  await expect(page.getByText("Standup 9", { exact: true })).toBeVisible();
  await bottom();
  await page.getByRole("button", { name: /next/i }).click();
  await expect(page.getByText("Standup 10", { exact: true })).toBeVisible();
  await expect.poll(y).toBe(0);
  await bottom();
  await page.getByRole("button", { name: /prev/i }).click();
  await expect(page.getByText("Standup 0", { exact: true })).toBeVisible();
  await expect.poll(y).toBe(0);

  // A fresh tab opened straight on a scrolled meeting page has no view to return to.
  const long = Array.from({ length: 60 }, (_, i) => `- point ${i}`).join("\n");
  await page.route(/\/api\/meetings\/[0-9a-f-]+(\?|$)/, (r) =>
    r.fulfill({ json: { ...MEETING, notes_md: long, enriched_md: `## Notes\n\n${long}\n` } }),
  );
  await page.evaluate(() => sessionStorage.clear());
  await page.goto(`/meeting/${MEETING.id}/post`);
  await expect(page.getByText("point 59")).toBeVisible();
  await bottom();
  expect(await y()).toBeGreaterThan(0);
  await page.getByRole("link", { name: "Library" }).click();
  await expect(page).toHaveURL(/\/$/);
  await expect.poll(y).toBe(0);
});
