import { test, expect, type Page } from "@playwright/test";

const DAY = 86_400_000;
const NOW = Date.now();

interface Row {
  id: string;
  title: string;
  started_at: number;
  archived_at?: string;
}

function full(m: Row) {
  return {
    ...m,
    ended_at: m.started_at + 60_000,
    notes_md: "",
    enriched_md: "notes",
    transcript_json: "[]",
    starred: false,
    has_enriched: true,
    stt_engine: "local · small.en",
    llm_model: null,
    template: null,
    labels: [],
    created_at: new Date(m.started_at).toISOString(),
    updated_at: new Date(m.started_at).toISOString(),
  };
}

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

// A stateful stand-in for the archive routes: PATCH flips one meeting, POST /archive archives by date.
async function mockBackend(page: Page, rows: Row[]) {
  await page.routeWebSocket(/\/ws(\/|\?|$)/, () => {});
  await page.route("**/api/session-token", (r) => r.fulfill({ json: { token: "e2e-token" } }));
  await page.route(/\/api\/settings(\?|$)/, (r) => r.fulfill({ json: SETTINGS }));
  await page.route("**/api/audio/permission", (r) =>
    r.fulfill({ json: { screen_recording: "granted", microphone: "granted" } }),
  );
  await page.route(/\/api\/labels(\?|$)/, (r) => r.fulfill({ json: [] }));
  const list = (url: URL) => {
    const archived = url.searchParams.get("archived") === "true";
    const q = (url.searchParams.get("q") ?? "").toLowerCase();
    const hits = rows
      .filter((m) => (m.archived_at != null) === archived && m.title.toLowerCase().includes(q))
      .sort((a, b) => b.started_at - a.started_at);
    return { meetings: hits.map(full), total: hits.length };
  };
  await page.route(/\/api\/meetings\/[0-9a-z-]+(\?|$)/, async (r) => {
    const id = new URL(r.request().url()).pathname.split("/").pop()!;
    const m = rows.find((x) => x.id === id)!;
    if (r.request().method() === "PATCH") {
      const { archived } = r.request().postDataJSON();
      if (!archived) delete m.archived_at;
      else m.archived_at ??= new Date().toISOString();
    }
    return r.fulfill({ json: full(m) });
  });
  await page.route(/\/api\/meetings(\/search)?(\?|$)/, (r) =>
    r.fulfill({ json: list(new URL(r.request().url())) }),
  );
  await page.route("**/api/meetings/archive", (r) => {
    const { before, preview } = r.request().postDataJSON();
    const hit = rows.filter((m) => m.archived_at == null && m.started_at < before);
    if (!preview) for (const m of hit) m.archived_at = new Date().toISOString();
    return r.fulfill({ json: { count: hit.length } });
  });
  await page.route("**/api/meetings/active", (r) => r.fulfill({ json: null }));
  await page.route("**/api/meetings/detected", (r) => r.fulfill({ json: null }));
}

const card = (page: Page, title: string) => page.locator("a").filter({ hasText: title });

test("archive from a card, find it in the Archive view and its search, and unarchive it", async ({ page }) => {
  const rows: Row[] = [
    { id: "m1", title: "Standup", started_at: NOW - DAY },
    { id: "m2", title: "Design review", started_at: NOW - 2 * DAY },
    { id: "m3", title: "Budget sync", started_at: NOW - 3 * DAY },
  ];
  await mockBackend(page, rows);
  await page.goto("/");
  await expect(page.getByText("3 meetings · all on this Mac")).toBeVisible();

  await card(page, "Design review").getByRole("button", { name: "Meeting actions" }).click();
  await page.getByRole("menuitem", { name: "Archive" }).click();
  await expect(card(page, "Design review")).toHaveCount(0);
  await expect(page.getByText("2 meetings · all on this Mac")).toBeVisible();

  await page.getByRole("link", { name: "Archive" }).click();
  await expect(page).toHaveURL(/\/archive$/);
  await expect(page.getByRole("heading", { name: "Archive" })).toBeVisible();
  await expect(page.getByText("1 archived meeting")).toBeVisible();
  await expect(card(page, "Design review")).toBeVisible();
  await expect(card(page, "Standup")).toHaveCount(0);

  const search = page.getByRole("textbox", { name: /search/i });
  await search.fill("standup");
  await expect(page.getByText("No matches")).toBeVisible();
  await search.fill("design");
  await expect(card(page, "Design review")).toBeVisible();
  await expect(page.getByText("1 archived meeting")).toBeVisible();
  await search.fill("");

  await card(page, "Design review").getByRole("button", { name: "Meeting actions" }).click();
  await page.getByRole("menuitem", { name: "Unarchive" }).click();
  await expect(page.getByText("No archived meetings")).toBeVisible();
  await expect(page.getByText("0 archived meetings")).toBeVisible();

  await page.getByRole("link", { name: "All meetings" }).click();
  await expect(card(page, "Design review")).toBeVisible();
});

test("multi-select archives every selected meeting with no confirmation", async ({ page }) => {
  const rows: Row[] = [
    { id: "m1", title: "Standup", started_at: NOW - DAY },
    { id: "m2", title: "Design review", started_at: NOW - 2 * DAY },
    { id: "m3", title: "Budget sync", started_at: NOW - 3 * DAY },
  ];
  await mockBackend(page, rows);
  await page.goto("/");
  for (const t of ["Standup", "Budget sync"]) {
    await card(page, t).hover();
    await card(page, t).getByRole("checkbox").click();
  }
  const bar = page.getByTestId("selection-action-bar");
  await bar.getByRole("button", { name: "Archive" }).click();
  await expect(bar).toHaveCount(0);
  await expect(card(page, "Standup")).toHaveCount(0);
  await expect(card(page, "Budget sync")).toHaveCount(0);
  await expect(page.getByText("1 meeting · all on this Mac")).toBeVisible();

  await page.goto("/archive");
  await expect(page.getByText("2 archived meetings")).toBeVisible();
  await card(page, "Standup").hover();
  await card(page, "Standup").getByRole("checkbox").click();
  await page.getByTestId("selection-action-bar").getByRole("button", { name: "Unarchive" }).click();
  await expect(page.getByText("1 archived meeting")).toBeVisible();
});

test("Archive older meetings names the count before archiving everything before the date", async ({ page }) => {
  const day = (iso: string) => new Date(`${iso}T12:00:00`).getTime();
  const rows: Row[] = [
    { id: "m1", title: "New year kickoff", started_at: day("2025-01-02") },
    { id: "m2", title: "Old planning", started_at: day("2024-12-30") },
    { id: "m3", title: "Older planning", started_at: day("2024-06-01") },
    { id: "m4", title: "Already archived", started_at: day("2024-01-01"), archived_at: "2024-02-01T00:00:00Z" },
  ];
  await mockBackend(page, rows);
  await page.goto("/archive");
  await expect(page.getByText("1 archived meeting")).toBeVisible();

  await page.getByRole("button", { name: "Archive older meetings" }).click();
  const panel = page.getByRole("alertdialog", { name: "Archive older meetings" });
  const field = panel.getByLabel("Started before");
  await expect(field).toHaveAttribute("max", /^\d{4}-\d{2}-\d{2}$/);
  await field.fill("2025-01-01");
  await expect(panel.getByText("Archive 2 meetings that started before Jan 1, 2025?")).toBeVisible();

  await panel.getByRole("button", { name: "Archive", exact: true }).click();
  await expect(panel).toHaveCount(0);
  await expect(page.getByText("3 archived meetings")).toBeVisible();
  await expect(card(page, "Old planning")).toBeVisible();
  await expect(card(page, "New year kickoff")).toHaveCount(0);
});
