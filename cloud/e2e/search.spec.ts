import { expect, test, type Page } from "@playwright/test";
import { createClient } from "@supabase/supabase-js";
import { API, DEVICES, KEY, OWNER, PASSWORD, PIN, expectScreen, isPhone, onboard, reset, seed, shot, signIn, typePin } from "./helpers";

const rpc = "**/rest/v1/rpc/yogurt_search";

// Later tests use the owner the first one creates.
test.describe.configure({ mode: "serial" });
test.beforeAll(reset);

async function unlock(page: Page) {
  await signIn(page);
  await expectScreen(page, "Unlock your meetings");
  await typePin(page, PIN);
  await page.getByRole("button", { name: "Unlock" }).click();
  await expect(page.getByRole("button", { name: /edis update/ })).toBeVisible();
}

/** A push from a laptop, written the way seed() writes one. */
async function push(meetingId: string, body: object) {
  const owner = createClient(API, KEY);
  await owner.auth.signInWithPassword({ email: OWNER, password: PASSWORD });
  const { data: id } = await owner.rpc("yogurt_start_push", { device_id: DEVICES.a });
  const { error } = await owner.from("yogurt_versions").insert({ meeting_id: meetingId, push_id: id, device_id: DEVICES.a, body: { v: 1, meeting: { id: meetingId, ended_at: null, enriched_md: null, transcript_json: "[]", ...body } } });
  expect(error).toBeNull();
  expect((await owner.rpc("yogurt_search_catch_up")).error).toBeNull();
}

const slow = (page: Page) =>
  page.route(rpc, async (r) => {
    await new Promise((done) => setTimeout(done, 700));
    await r.continue();
  });

test("search the live library by transcript, within a device, with a cap message, and clear it", async ({ page }, info) => {
  await onboard(page);
  await seed();
  await page.reload();
  await expect(page.getByRole("button", { name: /edis update/ })).toBeVisible();

  // A word only one transcript has: the library stays up, dimmed and inert, until the answer arrives.
  const search = page.getByRole("textbox", { name: "Search notes and transcripts" });
  await expect(search).toHaveAttribute("autocorrect", "off");
  await expect(search).toHaveAttribute("autocapitalize", "off");
  await expect(search).toHaveAttribute("spellcheck", "false");
  const busy = page.locator('[aria-busy="true"]');
  await slow(page);
  await search.fill("KOMBUCHA");
  await expect(page.getByRole("status")).toHaveText("Searching…");
  await expect(busy).toBeVisible();
  await expect(busy.getByRole("button", { name: /edis update/ })).toBeVisible();
  await shot(page, info, "t-search-pending");
  const hit = page.getByRole("button", { name: /Q4 planning sync/ });
  await expect(page.getByRole("status")).toHaveText("1 match");
  await expect(busy).toHaveCount(0);
  await expect(page.getByRole("button", { name: /edis update/ })).toHaveCount(0);
  await page.unroute(rpc);
  await expect(hit.locator("mark")).toHaveText("kombucha");
  await expect(hit).toContainText("bringing the kombucha to the offsite");
  await shot(page, info, "t-search");

  // The next query keeps the last answer on screen, dimmed, and never calls it a miss.
  await slow(page);
  await search.fill("migration");
  await expect(page.getByRole("status")).toHaveText("Searching…");
  await expect(busy.getByRole("button", { name: /Q4 planning sync/ })).toBeVisible();
  await expect(page.getByText("No meetings match")).toHaveCount(0);
  await shot(page, info, "t-search-pending-results");
  await expect(page.getByRole("status")).toHaveText("1 match");
  await page.unroute(rpc);

  // The device pill narrows the search on the server, and an empty result says so.
  await search.fill("KOMBUCHA");
  await expect(hit).toBeVisible();
  const devices = page.getByRole("group", { name: "Device" });
  await devices.getByRole("button", { name: "Laptop 1" }).click();
  await expect(page.getByRole("status")).toHaveText("No meetings match KOMBUCHA.");
  await expect(hit).toHaveCount(0);
  await shot(page, info, "t-search-empty");

  // After a miss, the next query keeps the empty answer dimmed instead of the whole library.
  await slow(page);
  await search.fill("KOMBUCHAS");
  await expect(page.getByRole("status")).toHaveText("Searching…");
  await expect(page.getByRole("button", { name: /edis update/ })).toHaveCount(0);
  await expect(page.getByRole("status")).toHaveText("No meetings match KOMBUCHAS.");
  await page.unroute(rpc);
  await devices.getByRole("button", { name: "All devices" }).click();
  await search.fill("KOMBUCHA");
  await expect(hit).toBeVisible();

  // A lone quote is no word, and the status line says so plainly.
  await search.fill('"');
  await expect(page.getByRole("status")).toHaveText("No meetings match.");

  // A full page of results says so.
  await page.route(rpc, async (r) => {
    const res = await r.fetch();
    await r.fulfill({ response: res, json: (await res.json()).map((x: object) => ({ ...x, more: true })) });
  });
  await search.fill("ship");
  await expect(page.getByRole("status")).toHaveText("Showing the newest matches. Add words to narrow.");
  await page.unroute(rpc);

  // A pasted URL wraps inside the page.
  await search.fill("https://example.com/" + "a".repeat(120));
  await expect(page.getByRole("status")).toContainText("No meetings match");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await shot(page, info, "t-search-long-url");

  // Clear empties the field, keeps focus in it, and restores the library.
  await page.getByRole("button", { name: "Clear search" }).click();
  await expect(search).toBeFocused();
  await expect(search).toHaveValue("");
  await expect(page.getByRole("button", { name: /edis update/ })).toBeVisible();
  await search.fill("edis");
  await search.press("Escape");
  await expect(search).toHaveValue("");

  // An earlier push is a recovery view of the whole library, and has no search.
  await page.goto("/#/at/2");
  await page.reload();
  await expect(page.getByText("Viewing", { exact: false })).toBeVisible();
  await expect(page.getByRole("button", { name: /Vendor review/ })).toBeVisible();
  await expect(search).toHaveCount(0);
});

test("a failed search says so, retries with focus kept, and a retyped query does not show the old error", async ({ page }) => {
  await unlock(page);
  const search = page.getByRole("textbox", { name: "Search notes and transcripts" });
  await page.route(rpc, (r) => r.abort());
  await search.fill("kombucha");
  await expect(page.getByRole("alert")).toContainText("Search failed");
  await expect(page.getByText("No meetings match")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /edis update/ })).toHaveCount(0);
  await page.unroute(rpc);
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(search).toBeFocused();
  await expect(page.getByRole("button", { name: /Q4 planning sync/ })).toBeVisible();

  await page.route(rpc, (r) => r.abort());
  await search.fill("kombuch");
  await expect(page.getByRole("alert")).toContainText("Search failed");
  await page.unroute(rpc);
  await slow(page);
  await search.fill("kombucha");
  await expect(page.getByRole("status")).toHaveText("Searching…");
  await expect(page.getByRole("alert")).toHaveCount(0);
  await page.unroute(rpc);
});

test("a hit missing from the loaded library or newer than it offers a reload that keeps the page", async ({ page }, info) => {
  await unlock(page);
  const search = page.getByRole("textbox", { name: "Search notes and transcripts" });
  const missing = page.getByText("This meeting isn't loaded on this page yet.");
  const newer = page.getByText("A newer version was pushed.");
  const library = "**/rest/v1/rpc/yogurt_library";

  // Tapping one hit and then another leaves the second open, since nothing waits on the network.
  if (!isPhone(page)) {
    await search.fill("ship");
    await expect(page.getByRole("status")).toHaveText("2 matches");
    await page.getByRole("button", { name: /edis update/ }).click();
    await page.getByRole("button", { name: /Q4 planning sync/ }).click();
    await page.waitForTimeout(500);
    await expect(page.getByRole("heading", { name: "Q4 planning sync" })).toBeVisible();
    await expect(missing).toHaveCount(0);
    await expect(newer).toHaveCount(0);
    await search.fill("");
  }

  // A meeting the loaded library lacks opens with its title and says so. A failed reload says so too and keeps the page.
  await push("m-new", { title: "Fresh meeting", started_at: Date.now() - 1000, notes_md: "Quokka notes." });
  await search.fill("quokka");
  await page.getByRole("button", { name: /Fresh meeting/ }).click();
  await expect(page.getByRole("heading", { name: "Fresh meeting" })).toBeVisible();
  await expect(missing).toBeVisible();
  await expect(newer).toHaveCount(0);
  await shot(page, info, "t-search-missing");
  await page.route(library, (r) => r.abort());
  await page.getByRole("button", { name: "Reload" }).click();
  await expect(page.getByRole("alert")).toContainText("Could not reload.");
  await expect(page.getByRole("heading", { name: "Fresh meeting" })).toBeVisible();
  await page.unroute(library);

  // Retry keeps the page up while it loads, then fills the meeting in.
  await page.route(library, async (r) => {
    await new Promise((done) => setTimeout(done, 700));
    await r.continue();
  });
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(page.getByRole("button", { name: "Reloading…" })).toBeDisabled();
  await expect(page.getByRole("heading", { name: "Fresh meeting" })).toBeVisible();
  await expect(page.getByText("Quokka notes.")).toBeVisible();
  await expect(missing).toHaveCount(0);
  await expect(page).toHaveURL(/#\/m\//);
  await page.unroute(library);

  // A newer version of a loaded meeting opens the loaded copy with its own note.
  await push("m-standup", { title: "edis update (zebrafish)", started_at: Date.now(), notes_md: "Feed the zebrafish herd." });
  if (isPhone(page)) await page.getByRole("button", { name: /All devices/ }).click();
  await search.fill("zebrafish");
  await page.getByRole("button", { name: /edis update/ }).click();
  await expect(page.getByRole("heading", { name: "edis update (revised)" })).toBeVisible();
  await expect(page.getByText("Ship the edis dashboard.")).toBeVisible();
  await expect(newer).toBeVisible();
  await expect(missing).toHaveCount(0);
  await shot(page, info, "t-search-newer");
  await page.getByRole("button", { name: "Reload" }).click();
  await expect(page.getByText("Feed the zebrafish herd.")).toBeVisible();
  await expect(newer).toHaveCount(0);

  // The view as of an earlier push shows neither note, though the remembered hit is newer than its copy.
  await page.evaluate(() => (location.hash = location.hash.replace("#/m/", "#/at/1/m/")));
  if (!isPhone(page)) await expect(page.getByText("Viewing", { exact: false })).toBeVisible();
  await expect(page.getByRole("heading", { name: "edis update", exact: true })).toBeVisible();
  await expect(newer).toHaveCount(0);
  await expect(missing).toHaveCount(0);
});

test("a failed library load shows an error with Retry", async ({ page }) => {
  await page.route("**/rest/v1/rpc/yogurt_library", (r) => r.abort());
  await signIn(page);
  await expectScreen(page, "Unlock your meetings");
  await typePin(page, PIN);
  await page.getByRole("button", { name: "Unlock" }).click();
  await expect(page.getByRole("alert")).toContainText("Could not load your library");
  await page.unroute("**/rest/v1/rpc/yogurt_library");
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(page.getByRole("button", { name: /edis update/ })).toBeVisible();
});
