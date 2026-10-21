import { test, expect, type Page } from "@playwright/test";

function meeting(id: string, title: string, startedAt: number) {
  return {
    id,
    title,
    started_at: startedAt,
    ended_at: startedAt + 60_000,
    notes_md: "",
    enriched_md: "notes",
    transcript_json: "[]",
    starred: false,
    stt_engine: "local · small.en",
    llm_model: null,
    template: null,
    labels: [] as { id: string; name: string; color: string }[],
    created_at: new Date(startedAt).toISOString(),
    updated_at: new Date(startedAt).toISOString(),
  };
}

// Newest first, matching `GET /api/meetings`.
const MEETINGS = [
  meeting("m1", "Standup", 1_700_003_600_000),
  meeting("m2", "Design review", 1_700_003_500_000),
  meeting("m3", "1:1 with Sam", 1_700_003_400_000),
];

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

async function mockBackend(page: Page, deletedIds: Set<string>) {
  // The Shell's app-wide socket would otherwise hit Vite's proxy with no backend behind it.
  await page.routeWebSocket(/\/ws(\/|\?|$)/, () => {});
  await page.route("**/api/session-token", (r) =>
    r.fulfill({ json: { token: "e2e-token" } }),
  );
  await page.route(/\/api\/settings(\?|$)/, (r) => r.fulfill({ json: SETTINGS }));
  await page.route("**/api/audio/permission", (r) =>
    r.fulfill({ json: { screen_recording: "granted", microphone: "granted" } }),
  );
  await page.route(/\/api\/labels(\?|$)/, (r) => r.fulfill({ json: [] }));
  // Register before the list route (Playwright matches the most recent route first).
  await page.route(/\/api\/meetings\/[0-9a-z-]+(\?|$)/, (r) => {
    const req = r.request();
    if (req.method() === "DELETE") {
      const id = new URL(req.url()).pathname.split("/").pop()!;
      deletedIds.add(id);
      return r.fulfill({ status: 204 });
    }
    return r.fulfill({ json: MEETINGS[0] });
  });
  await page.route(/\/api\/meetings(\?|$)/, (r) =>
    r.fulfill({ json: MEETINGS.filter((m) => !deletedIds.has(m.id)) }),
  );
  // Registered last so these literal matches win over the id regex above.
  await page.route("**/api/meetings/active", (r) => r.fulfill({ json: null }));
  await page.route("**/api/meetings/detected", (r) => r.fulfill({ json: null }));
}

function cardFor(page: Page, title: string) {
  return page.locator("a").filter({ hasText: title });
}

test("checkbox select, shift-click range, and Escape clears", async ({ page }) => {
  await mockBackend(page, new Set());
  await page.goto("/");
  const card1 = cardFor(page, "Standup");
  await expect(card1).toBeVisible();

  await card1.hover();
  const checkbox1 = card1.getByRole("checkbox");
  await expect(checkbox1).toBeVisible();
  await checkbox1.click();
  await expect(checkbox1).toBeChecked();

  await expect(page.getByTestId("selection-action-bar")).toBeVisible();
  await expect(page.getByText("1 selected")).toBeVisible();
  await page.screenshot({ path: "test-results/mtg-14-selection-state.png" });

  const card3 = cardFor(page, "1:1 with Sam");
  await card3.hover();
  await card3.getByRole("checkbox").click({ modifiers: ["Shift"] });
  await expect(page.getByText("3 selected")).toBeVisible();
  await page.screenshot({ path: "test-results/mtg-14-action-bar.png" });

  // Every checkbox is pinned visible and actually checked, not just accessibly named (a DOM/state desync once left the box unchecked while its name said "Deselect").
  const checkbox2 = page.getByRole("checkbox", { name: "Deselect Design review" });
  await expect(checkbox2).toBeVisible();
  await expect(checkbox2).toBeChecked();
  await expect(checkbox1).toBeChecked();
  await expect(card3.getByRole("checkbox")).toBeChecked();

  await page.keyboard.press("Escape");
  await expect(page.getByTestId("selection-action-bar")).toHaveCount(0);
});

test("Select all selects every visible meeting", async ({ page }) => {
  await mockBackend(page, new Set());
  await page.goto("/");
  const card1 = cardFor(page, "Standup");
  await card1.hover();
  await card1.getByRole("checkbox").click();

  await page.getByRole("button", { name: "Select all" }).click();
  await expect(page.getByText("3 selected")).toBeVisible();
});

test("Delete requires a confirm step, then removes the selected meetings", async ({ page }) => {
  const deletedIds = new Set<string>();
  await mockBackend(page, deletedIds);
  await page.goto("/");

  const card1 = cardFor(page, "Standup");
  await card1.hover();
  await card1.getByRole("checkbox").click();
  const card2 = cardFor(page, "Design review");
  await card2.hover();
  await card2.getByRole("checkbox").click();

  await page.getByRole("button", { name: "Delete" }).click();
  await expect(page.getByTestId("bulk-delete-confirm-popover")).toBeVisible();
  expect(deletedIds.size).toBe(0);

  await page.getByRole("menuitem", { name: "Delete?" }).click();

  await expect(page.getByTestId("selection-action-bar")).toHaveCount(0);
  await expect(page.getByText("Standup")).toHaveCount(0);
  await expect(page.getByText("Design review")).toHaveCount(0);
  await expect(page.getByText("1:1 with Sam")).toBeVisible();
  expect([...deletedIds].sort()).toEqual(["m1", "m2"]);
});
