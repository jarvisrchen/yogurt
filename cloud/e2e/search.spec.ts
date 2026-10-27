import { expect, test, type Page } from "@playwright/test";
import { createClient } from "@supabase/supabase-js";
import { API, DEVICES, KEY, OWNER, PASSWORD, PIN, expectScreen, onboard, reset, seed, shot, signIn, typePin } from "./helpers";

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

  // A full page of results says which ones it shows.
  await page.route(rpc, async (r) => {
    const res = await r.fetch();
    await r.fulfill({ response: res, json: (await res.json()).map((x: object) => ({ ...x, more: true })) });
  });
  await search.fill("ship");
  await expect(page.getByRole("status")).toHaveText("Matches 1–2");
  await expect(page.getByRole("button", { name: "Next" })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Previous" })).toBeDisabled();
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

test("a search hit opens through yogurt_meeting with its live body, and a failed fetch offers Retry", async ({ page }, info) => {
  await unlock(page);
  const search = page.getByRole("textbox", { name: "Search notes and transcripts" });
  const meetingRpc = "**/rest/v1/rpc/yogurt_meeting";
  const loading = page.getByText("Loading…");

  // A meeting pushed after the page loaded opens with its body, and the header shows while the body loads.
  await push("m-new", { title: "Fresh meeting", started_at: Date.now() - 1000, notes_md: "Quokka notes." });
  await search.fill("quokka");
  await page.getByRole("button", { name: /Fresh meeting/ }).click();
  await expect(page.getByRole("heading", { name: "Fresh meeting" })).toBeVisible();
  await expect(page.getByText("Quokka notes.")).toBeVisible();
  await expect(page.getByText("isn't loaded on this page")).toHaveCount(0);

  // A failed fetch says so and keeps the header. Retry fills the body in.
  await page.route(meetingRpc, (r) => r.abort());
  await page.reload();
  await expect(page.getByRole("alert")).toContainText("Could not load this meeting.");
  await page.unroute(meetingRpc);
  await page.route(meetingRpc, async (r) => {
    await new Promise((done) => setTimeout(done, 700));
    await r.continue();
  });
  await page.getByRole("button", { name: "Retry" }).click();
  await expect(loading).toBeVisible();
  await shot(page, info, "t-meeting-loading");
  await expect(page.getByText("Quokka notes.")).toBeVisible();
  await expect(page).toHaveURL(/#\/m\//);
  await page.unroute(meetingRpc);

  // A hit for a meeting edited since the page loaded opens the live version, with no note about it.
  await push("m-standup", { title: "edis update (zebrafish)", started_at: Date.now(), notes_md: "Feed the zebrafish herd." });
  await page.goto("/");
  await search.fill("zebrafish");
  await page.getByRole("button", { name: /edis update/ }).click();
  await expect(page.getByRole("heading", { name: "edis update (zebrafish)" })).toBeVisible();
  await expect(page.getByText("Feed the zebrafish herd.")).toBeVisible();
  await expect(page.getByText("A newer version was pushed.")).toHaveCount(0);

  // The same meeting as of an earlier push is that earlier version.
  await page.evaluate(() => (location.hash = location.hash.replace("#/m/", "#/at/1/m/")));
  await expect(page.getByRole("heading", { name: "edis update", exact: true })).toBeVisible();
  await expect(page.getByText("Ship the edis dashboard.")).toBeVisible();
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
