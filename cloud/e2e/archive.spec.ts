import { expect, test, type Page } from "@playwright/test";
import { createClient } from "@supabase/supabase-js";
import { API, DEVICES, KEY, OWNER, PASSWORD, PIN, expectScreen, isPhone, onboard, reset, seed, seedBulk, shot, signIn, typePin } from "./helpers";

// Later tests use the owner and the meetings the first one creates.
test.describe.configure({ mode: "serial" });
test.beforeAll(reset);

const range = (page: Page) => page.getByTestId("pager-range");
const filter = (page: Page, name: string) => page.getByRole("group", { name: "Filter" }).getByRole("button", { name });
const device = (page: Page, name: string) => page.getByRole("group", { name: "Device" }).getByRole("button", { name });

test("the Archive pill lists and searches only archived meetings, under All devices or one", async ({ page }, info) => {
  await onboard(page);
  await seed();
  await seedBulk(3, { prefix: "Archived review", kb: 1, archivedAt: new Date().toISOString() });
  await page.reload();

  // All hides the archive, and the total counts only what it lists.
  await expect(page.getByRole("button", { name: /edis update/ })).toBeVisible();
  await expect(filter(page, "All")).toHaveAttribute("aria-pressed", "true");
  await expect(range(page)).toHaveText("1–2 of 2");
  await expect(page.getByRole("button", { name: /^Archived review/ })).toHaveCount(0);
  await shot(page, info, "a-all");

  await filter(page, "Archive").click();
  await expect(filter(page, "Archive")).toHaveAttribute("aria-pressed", "true");
  await expect(range(page)).toHaveText("1–3 of 3");
  await expect(page.getByRole("button", { name: /^Archived review/ })).toHaveCount(3);
  await expect(page.getByRole("button", { name: /edis update/ })).toHaveCount(0);
  await expect(device(page, "All devices")).toHaveAttribute("aria-pressed", "true");
  await shot(page, info, "a-archive");

  // An archived meeting opens like any other, and on a phone its Back names the Archive list it returns to.
  await page.getByRole("button", { name: /^Archived review 02/ }).click();
  await expect(page.getByRole("heading", { name: "Archived review 02" })).toBeVisible();
  if (isPhone(page)) {
    await page.getByRole("button", { name: "← Archive" }).click();
    await expect(filter(page, "Archive")).toHaveAttribute("aria-pressed", "true");
  } else {
    await page.getByRole("link", { name: "Library" }).click();
  }

  // Search in the archive finds only archived meetings, and All searches everything else.
  const search = page.getByRole("textbox", { name: "Search notes and transcripts" });
  await search.fill("roadmap");
  await expect(page.getByRole("status")).toHaveText("3 matches");
  await expect(page.getByRole("button", { name: /Q4 planning sync/ })).toHaveCount(0);
  await shot(page, info, "a-archive-search");
  await filter(page, "All").click();
  await expect(page.getByRole("status")).toHaveText("1 match");
  await expect(page.getByRole("button", { name: /Q4 planning sync/ })).toBeVisible();
  await search.fill("");

  // A device with nothing archived shows the archive's empty state.
  await filter(page, "Archive").click();
  await device(page, "Laptop 2").click();
  await expect(page.getByText("No archived meetings.", { exact: false })).toBeVisible();
  await shot(page, info, "a-archive-empty");
  await device(page, "Laptop 1").click();
  await expect(range(page)).toHaveText("1–3 of 3");
});

test("with every meeting archived, the library stays reachable through the Archive pill", async ({ page }) => {
  const owner = createClient(API, KEY);
  await owner.auth.signInWithPassword({ email: OWNER, password: PASSWORD });
  for (const [dev, id, title] of [[DEVICES.a, "m-standup", "edis update (revised)"], [DEVICES.b, "m-planning", "Q4 planning sync"]]) {
    const { data: push } = await owner.rpc("yogurt_start_push", { device_id: dev });
    const body = { v: 1, meeting: { id, title, started_at: Date.now() - 3_600_000, ended_at: null, notes_md: "", enriched_md: null, transcript_json: "[]", archived_at: new Date().toISOString() } };
    expect((await owner.from("yogurt_versions").insert({ meeting_id: id, push_id: push, device_id: dev, body })).error).toBeNull();
  }
  await owner.auth.signOut();

  await signIn(page);
  await expectScreen(page, "Unlock your meetings");
  await typePin(page, PIN);
  await page.getByRole("button", { name: "Unlock" }).click();
  await expect(page.getByText("No meetings here.")).toBeVisible();
  await expect(page.getByRole("heading", { name: /Waiting for the first push/ })).toHaveCount(0);
  await filter(page, "Archive").click();
  await expect(range(page)).toHaveText("1–5 of 5");
});
