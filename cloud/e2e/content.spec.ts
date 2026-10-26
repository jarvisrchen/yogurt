import { expect, test } from "@playwright/test";
import { createClient } from "@supabase/supabase-js";
import { API, DEVICES, KEY, OWNER, PASSWORD, LATE, PIN, expectScreen, isPhone, onboard, reset, seed, shot, typePin } from "./helpers";

test.beforeAll(reset);

test("read the library by device, open a meeting, browse History, and view an earlier push", async ({ page, browser }, info) => {
  const phone = isPhone(page);
  await onboard(page);
  await seed();
  await page.reload();

  // Library: two devices, grouped by day, newest first.
  const library = page.getByRole("button", { name: /edis update/ });
  await expect(library).toBeVisible();
  await expect(page.getByRole("group", { name: "Device" }).getByRole("button")).toHaveText(["All devices", "Laptop 1", "Laptop 2"]);
  await expect(page.getByRole("heading", { name: "Today" })).toBeVisible();
  await expect(page.getByRole("button", { name: /Vendor review/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /1:1 with Steven/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Q4 planning sync/ })).toBeVisible();
  await expect(page.getByText("2:59 PM · 11 min")).toBeVisible();
  await expect(page.getByText("read-only", { exact: true })).toBeVisible();
  await shot(page, info, phone ? "m-library" : "o-desktop-library-empty-pane");

  // The device filter.
  const devices = page.getByRole("group", { name: "Device" });
  await devices.getByRole("button", { name: "Laptop 2" }).click();
  await expect(page.getByRole("button", { name: /edis update/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Q4 planning sync/ })).toBeVisible();
  await devices.getByRole("button", { name: "Laptop 1" }).click();
  await expect(page.getByRole("button", { name: /Q4 planning sync/ })).toHaveCount(0);
  await devices.getByRole("button", { name: "All devices" }).click();

  // A meeting with notes, an enhanced version, and a transcript.
  await page.getByRole("button", { name: /Q4 planning sync/ }).click();
  await expect(page.getByRole("heading", { name: "Q4 planning sync" })).toBeVisible();
  await expect(page.getByRole("tab")).toHaveText(["Notes", "Enhanced", "Transcript"]);
  await expect(page.getByRole("tab", { name: "Enhanced" })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByText("Steven owns the migration plan")).toBeVisible();
  await expect(page.getByText(/^Pushed from Laptop 2 · \d+\/\d+\/\d+ \d+:\d\d [AP]M$/)).toBeVisible();
  await shot(page, info, phone ? "n-meeting" : "o-desktop-meeting");
  await page.getByRole("tab", { name: "Notes" }).click();
  await expect(page.getByText("Steven owns the migration plan")).toHaveCount(0);
  await expect(page.getByText("Ship the pricing page before the holiday freeze")).toBeVisible();
  await page.getByRole("tab", { name: "Transcript" }).click();
  await expect(page.getByText("Can you own the migration plan?")).toBeVisible();
  await shot(page, info, phone ? "n-meeting-transcript" : "o-desktop-transcript");

  // The meeting is in the URL: reload reopens it, and the back gesture returns to the library without leaving the app.
  await expect(page).toHaveURL(/#\/m\//);
  await page.reload();
  await expect(page.getByRole("heading", { name: "Q4 planning sync" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Transcript" })).toBeVisible();
  await page.goBack();
  await expect(page).not.toHaveURL(/#\/m\//);
  await expect(page.getByRole("heading", { name: "Q4 planning sync" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /edis update/ })).toBeVisible();
  await page.goForward();
  await expect(page.getByRole("heading", { name: "Q4 planning sync" })).toBeVisible();
  await page.goBack();

  // A meeting with notes only shows no tab bar.
  await page.getByRole("button", { name: /edis update/ }).click();
  await expect(page.getByRole("tab")).toHaveCount(0);
  await expect(page.getByText("Ship the edis dashboard.")).toBeVisible();
  if (phone) await page.getByRole("button", { name: "← All devices" }).click();

  // History lists every push, newest first, with combined counts.
  await page.getByRole("button", { name: "Settings" }).click();
  await expectScreen(page, "Settings");
  await page.getByRole("button", { name: /History/ }).click();
  await expectScreen(page, "History");
  const rows = page.locator("main button").filter({ hasText: /meeting/ });
  await expect(rows).toHaveCount(4);
  await expect(rows.nth(0)).toContainText("1 meeting removed");
  await expect(rows.nth(0)).toContainText("Laptop 2");
  await expect(rows.nth(1)).toContainText("1 meeting updated, 1 removed");
  await expect(rows.nth(1)).toContainText("Laptop 1");
  await expect(rows.nth(2)).toContainText("2 meetings updated");
  await expect(rows.nth(3)).toContainText("2 meetings updated");
  await shot(page, info, "q-history");
  await page.getByRole("group", { name: "Device" }).getByRole("button", { name: "Laptop 2" }).click();
  await expect(rows).toHaveCount(2);
  await page.getByRole("group", { name: "Device" }).getByRole("button", { name: "All devices" }).click();

  // Push 2 is the library before the edit and the two removals.
  await rows.nth(2).click();
  await expect(page).toHaveURL(/#\/at\/\d+$/);
  await page.reload();
  await expect(page.getByText("Viewing", { exact: false })).toBeVisible();
  await expect(page.getByRole("button", { name: "Latest" })).toBeVisible();
  for (const title of [/^edis update 2:59 PM/, /Vendor review/, /Q4 planning sync/, /1:1 with Steven/]) await expect(page.getByRole("button", { name: title })).toBeVisible();
  await expect(page.getByRole("button", { name: /revised/ })).toHaveCount(0);
  await shot(page, info, "r-library-as-of");

  // A meeting opened from there is that version, and Latest goes back to the live library.
  await page.getByRole("button", { name: /Vendor review/ }).click();
  await expect(page.getByText("Ask for the SOC 2 report.")).toBeVisible();
  await expect(page).toHaveURL(/#\/at\/\d+\/m\//);
  if (phone) {
    await page.getByRole("button", { name: "← All devices" }).click();
    await expect(page).toHaveURL(/#\/at\/\d+$/);
  }
  await page.getByRole("button", { name: "Latest" }).click();
  await expect(page.getByRole("button", { name: /edis update \(revised\)/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /Vendor review/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Latest" })).toHaveCount(0);
  await shot(page, info, "library-latest");

  // A deep link survives sign-in and unlock in a browser that has no session.
  const other = await browser.newContext({ baseURL: info.project.use.baseURL, viewport: page.viewportSize()!, colorScheme: info.project.use.colorScheme });
  const fresh = await other.newPage();
  await fresh.goto(`/#/m/${DEVICES.b}/m-planning`);
  await fresh.getByLabel("Email").fill(OWNER);
  await fresh.getByLabel("Password").fill(PASSWORD);
  await fresh.getByRole("button", { name: "Sign in" }).click();
  await expectScreen(fresh, "Unlock your meetings");
  await typePin(fresh, PIN);
  await fresh.getByRole("button", { name: "Unlock" }).click();
  await expect(fresh.getByRole("heading", { name: "Q4 planning sync" })).toBeVisible();
  await other.close();

  // A token the server no longer knows keeps the deep link: unlocking lands on the meeting, not the library.
  const stored = await page.evaluate(() => JSON.parse(localStorage.getItem("yogurt-cloud-browser")!).token as string);
  const owner = createClient(API, KEY);
  await owner.auth.signInWithPassword({ email: OWNER, password: PASSWORD });
  expect((await owner.rpc("yogurt_lock", { token: stored })).data).toBe(true);
  await page.goto(`/#/m/${DEVICES.b}/m-planning`);
  await page.reload();
  await expectScreen(page, "Unlock your meetings");
  await typePin(page, PIN);
  await page.getByRole("button", { name: "Unlock" }).click();
  await expect(page.getByRole("heading", { name: "Q4 planning sync" })).toBeVisible();
  await expect(page).toHaveURL(/#\/m\//);
});

test("a Cloud with sign-ups closed says so on Create login", async ({ page }, info) => {
  await page.route("**/auth/v1/signup*", (r) =>
    r.fulfill({ status: 422, contentType: "application/json", body: JSON.stringify({ code: 422, error_code: "signup_disabled", msg: "Signups not allowed for this instance" }) }),
  );
  await page.goto("/");
  await page.getByRole("button", { name: "Create your Cloud login" }).click();
  await page.getByLabel("Email").fill(OWNER);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Create login" }).click();
  await expect(page.getByRole("alert")).toHaveText("This Cloud isn't taking new logins. Sign in with the owner account.");
  await shot(page, info, "e-create-closed");
});

test("signing out of the webapp leaves a laptop's session alive", async ({ page }) => {
  await reset();
  await onboard(page);
  const laptop = createClient(API, KEY, { auth: { persistSession: false, autoRefreshToken: false } });
  const { error: signInError } = await laptop.auth.signInWithPassword({ email: OWNER, password: PASSWORD });
  expect(signInError).toBeNull();

  await page.getByRole("button", { name: "Settings" }).click();
  await page.getByRole("button", { name: "Sign out" }).click();
  await expectScreen(page, "Sign in");

  const { error } = await laptop.auth.refreshSession();
  expect(error).toBeNull();
});

test("a transcript link in Enhanced scrolls the Transcript tab to that moment and tints the line", async ({ page }, info) => {
  await reset();
  await onboard(page);
  await seed();
  await page.reload();
  await page.getByRole("button", { name: /Q4 planning sync/ }).click();
  await page.getByRole("tabpanel").locator("[data-transcript-link]").click();
  await expect(page.getByRole("tab", { name: "Transcript" })).toHaveAttribute("aria-selected", "true");
  const line = page.locator("[data-transcript-ts-sec]", { hasText: LATE.text });
  await expect(line).toBeInViewport({ ratio: 1 });
  await expect(line).toHaveClass(/transcript-highlight/);
  await shot(page, info, "s-transcript-link");
});
