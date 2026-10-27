import { expect, test, type Page } from "@playwright/test";
import { DEVICES, PIN, expectScreen, isPhone, onboard, reset, seedBulk, shot, signIn, typePin } from "./helpers";

// Later tests use the owner and the meetings the first one creates.
test.describe.configure({ mode: "serial" });
test.beforeAll(reset);

const range = (page: Page) => page.getByTestId("pager-range");
const rows = (page: Page) => page.getByRole("button", { name: /^Synthetic meeting/ });

/** Response bodies of a Cloud RPC, collected as the page makes them. */
function watch(page: Page, fn: string) {
  const seen: { status: number; text: string }[] = [];
  page.on("response", async (r) => {
    if (r.url().includes(`/rest/v1/rpc/${fn}`)) seen.push({ status: r.status(), text: await r.text().catch(() => "") });
  });
  return seen;
}

test("page the library, keep the page and size in the URL, and open a meeting from a later page", async ({ page }, info) => {
  await onboard(page);
  await seedBulk(45);
  await seedBulk(15, { device: DEVICES.b, name: "Laptop 2", prefix: "Synthetic review", kb: 4, skewMin: 90 });
  const libraryCalls = watch(page, "yogurt_library");
  const meetingCalls = watch(page, "yogurt_meeting");
  await page.reload();

  // The first page: 25 rows, the range, and a Previous that has nowhere to go.
  await expect(range(page)).toHaveText("1–25 of 60");
  await expect(page.getByRole("button", { name: /^Synthetic/ })).toHaveCount(25);
  await expect(page.getByRole("button", { name: "Previous" })).toBeDisabled();
  await shot(page, info, "p-library-pager");

  // The page response is a summary: titles and times, with no note or transcript text.
  const first = libraryCalls.find((c) => c.status === 200)!;
  expect(JSON.parse(first.text)).toHaveLength(25);
  expect(first.text).not.toContain("Notes for meeting");
  expect(first.text).not.toContain("transcript_json");
  expect(first.text.length).toBeLessThan(15_000);

  // Next and Previous move through the pages, and the URL follows.
  await page.getByRole("button", { name: "Next" }).click();
  await expect(range(page)).toHaveText("26–50 of 60");
  await expect(page).toHaveURL(/#\/p\/2$/);
  await page.getByRole("button", { name: "Next" }).click();
  await expect(range(page)).toHaveText("51–60 of 60");
  await expect(page.getByRole("button", { name: "Next" })).toBeDisabled();
  await expect(page.getByRole("button", { name: /^Synthetic/ })).toHaveCount(10);
  await page.getByRole("button", { name: "Previous" }).click();
  await expect(range(page)).toHaveText("26–50 of 60");

  // A reload lands on the same page, and a meeting opened from it loads its body alone.
  await page.reload();
  await expect(range(page)).toHaveText("26–50 of 60");
  const before = meetingCalls.length;
  await page.getByRole("button", { name: /^Synthetic meeting 30\b/ }).click();
  await expect(page.getByRole("heading", { name: "Synthetic meeting 30" })).toBeVisible();
  await expect(page.getByText(/Notes for meeting 30\./)).toBeVisible();
  await expect.poll(() => meetingCalls.length).toBe(before + 1);
  expect(meetingCalls.at(-1)!.text).toContain("Notes for meeting 30.");
  await expect(page).toHaveURL(/#\/p\/2\/m\//);
  await shot(page, info, "p-meeting-from-page-2");

  // Back returns to the same page. The page size is a choice kept in the URL, and a new size starts at page 1.
  if (isPhone(page)) await page.getByRole("button", { name: /All devices/ }).click();
  await expect(range(page)).toHaveText("26–50 of 60");
  await page.getByLabel("Meetings per page").selectOption("10");
  await expect(range(page)).toHaveText("1–10 of 60");
  await expect(page).toHaveURL(/#\/n\/10/);
  await page.getByRole("button", { name: "Next" }).click();
  await expect(range(page)).toHaveText("11–20 of 60");
  await page.reload();
  await expect(range(page)).toHaveText("11–20 of 60");
  await page.getByLabel("Meetings per page").selectOption("100");
  await expect(range(page)).toHaveText("1–60 of 60");
  await expect(page.getByRole("button", { name: "Next" })).toBeDisabled();
  await page.getByLabel("Meetings per page").selectOption("25");

  // A device narrows the pages and the total, and sends the view back to page 1.
  await page.getByRole("button", { name: "Next" }).click();
  await expect(range(page)).toHaveText("26–50 of 60");
  await page.getByRole("group", { name: "Device" }).getByRole("button", { name: "Laptop 2" }).click();
  await expect(range(page)).toHaveText("1–15 of 15");
  await expect(rows(page)).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Synthetic review/ })).toHaveCount(15);
  await page.getByRole("group", { name: "Device" }).getByRole("button", { name: "All devices" }).click();

  // A page past the end starts over instead of showing nothing.
  await page.goto("/#/p/99/n/25");
  await expect(range(page)).toHaveText("1–25 of 60");
  await expect(page).not.toHaveURL(/p\/99/);
});

test("search pages with Previous and Next, and a hit from a later page loads its own body", async ({ page }, info) => {
  await signIn(page);
  await expectScreen(page, "Unlock your meetings");
  await typePin(page, PIN);
  await page.getByRole("button", { name: "Unlock" }).click();
  await expect(range(page)).toBeVisible();
  const meetingCalls = watch(page, "yogurt_meeting");

  const search = page.getByRole("textbox", { name: "Search notes and transcripts" });
  await search.fill("roadmap");
  await expect(page.getByRole("status")).toHaveText("Matches 1–25");
  await expect(page.getByRole("button", { name: "Previous" })).toBeDisabled();
  await shot(page, info, "p-search-page-1");
  await page.getByRole("button", { name: "Next" }).click();
  await expect(page.getByRole("status")).toHaveText("Matches 26–50");
  await page.getByRole("button", { name: "Next" }).click();
  await expect(page.getByRole("status")).toHaveText("Matches 51–60");
  await expect(page.getByRole("button", { name: "Next" })).toBeDisabled();
  await shot(page, info, "p-search-page-3");

  // The hit opens through yogurt_meeting, though the library page never held it.
  await page.getByRole("button", { name: /^Synthetic meeting 45\b/ }).click();
  await expect(page.getByText(/Notes for meeting 45\./)).toBeVisible();
  await expect.poll(() => meetingCalls.length).toBeGreaterThan(0);
  expect(meetingCalls.at(-1)!.status).toBe(200);

  // A new query starts at the first page again.
  if (isPhone(page)) await page.getByRole("button", { name: /All devices/ }).click();
  await search.fill("meeting 07");
  await expect(page.getByRole("status")).toHaveText("2 matches");
});
