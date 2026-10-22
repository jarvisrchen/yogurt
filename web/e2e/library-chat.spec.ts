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
    r.fulfill({ json: [MEETING] }),
  );
  await page.route("**/api/chat", async (r) => {
    chatBodies.push(r.request().postDataJSON());
    await r.fulfill({
      contentType: "application/x-ndjson",
      body:
        [
          { sources: [{ id: MEETING.id, title: MEETING.title, started_at: MEETING.started_at }] },
          { delta: "The Q3 budget lands at " },
          { delta: "1.2 million [1]." },
        ]
          .map((f) => JSON.stringify(f))
          .join("\n") + "\n",
    });
  });
}

const chatBodies: unknown[] = [];

test("asking across meetings streams an answer with a chip to the cited meeting", async ({
  page,
}) => {
  await mockBackend(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Ask across meetings" }).click();
  const chat = page.getByRole("region", { name: "Ask across meetings chat" });
  await chat.getByPlaceholder("Ask across meetings…").fill("What did we decide about the budget?");
  await chat.getByPlaceholder("Ask across meetings…").press("Enter");

  await expect(chat.getByText("The Q3 budget lands at 1.2 million [1].")).toBeVisible();
  expect(chatBodies.at(-1)).toEqual({
    messages: [{ role: "user", content: "What did we decide about the budget?" }],
  });
  await page.screenshot({ path: test.info().outputPath("library-chat.png") });

  await chat.getByRole("link", { name: /Quarterly planning/ }).click();
  await expect(page).toHaveURL(new RegExp(`/meeting/${MEETING.id}/post$`));
});
