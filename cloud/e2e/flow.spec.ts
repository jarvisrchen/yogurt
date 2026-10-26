import { expect, test } from "@playwright/test";
import { OWNER, PASSWORD, confirmationLink, expectScreen, reset, shot, signIn, typePin } from "./helpers";

test.beforeAll(reset);

test("create the login, choose a PIN, lock, unlock, forget, change the PIN, lock out", async ({ page, browser }, info) => {
  await page.goto("/");
  await expectScreen(page, "Sign in");
  await shot(page, info, "e-signin");

  await page.getByRole("button", { name: "Create your Cloud login" }).click();
  await expectScreen(page, "Create your Cloud login");
  await page.getByLabel("Email").fill(OWNER);
  await page.getByLabel("Password").fill(PASSWORD);
  await shot(page, info, "e-create");
  await page.getByRole("button", { name: "Create login" }).click();
  await expectScreen(page, "Check your email");
  await shot(page, info, "f-check-email");

  let statusCalls = 0;
  page.on("request", (r) => r.url().includes("/rpc/yogurt_status") && statusCalls++);
  await page.goto(await confirmationLink());
  await expectScreen(page, /Choose a 4 or 6 digit PIN/);
  await page.waitForTimeout(500);
  expect(statusCalls).toBe(1);
  await page.getByLabel("PIN", { exact: true }).fill("246810");
  await page.getByLabel("Confirm PIN").fill("246811");
  await expect(page.getByText("The two PINs differ.")).toBeVisible();
  await shot(page, info, "g-choose-pin-mismatch");
  await page.getByLabel("Confirm PIN").fill("246810");
  await shot(page, info, "g-choose-pin");
  await page.getByRole("button", { name: "Save PIN" }).click();

  await expectScreen(page, "No machine has pushed yet");
  await shot(page, info, "library-empty");

  await page.getByRole("button", { name: "Settings" }).click();
  await expectScreen(page, "Settings");
  await expect(page.getByText(/^This browser( ·|$)/)).toBeVisible();
  await shot(page, info, "p-settings");

  const theme = page.locator("html");
  const appearance = page.getByRole("group", { name: "Appearance" });
  const initial = await theme.getAttribute("data-theme");
  const flipped = initial === "dark" ? "Light" : "Dark";
  await appearance.getByRole("button", { name: flipped }).click();
  await expect(theme).toHaveAttribute("data-theme", flipped.toLowerCase());
  const paper = (await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--color-paper"))).trim();
  await expect(page.locator('meta[name="theme-color"]')).toHaveAttribute("content", paper);
  await shot(page, info, "p-settings-appearance");
  await page.reload();
  await expectScreen(page, "Settings");
  await expect(theme).toHaveAttribute("data-theme", flipped.toLowerCase());
  await expect(appearance.getByRole("button", { name: flipped })).toHaveAttribute("aria-pressed", "true");
  await appearance.getByRole("button", { name: "System" }).click();
  await expect(theme).toHaveAttribute("data-theme", initial!);

  // Lock, then a wrong PIN, then the right one with Remember this browser.
  await page.getByRole("button", { name: /^Lock/ }).first().click();
  await expectScreen(page, "Unlock your meetings");
  await expect(page.locator("i[data-filled]")).toHaveCount(6);
  await shot(page, info, "i-unlock");
  await typePin(page, "000000");
  await page.getByRole("button", { name: "Unlock" }).click();
  await expect(page.getByText("Wrong PIN. Try again.")).toBeVisible();
  await shot(page, info, "i-unlock-wrong");
  await typePin(page, "246810");
  await page.getByRole("button", { name: "Unlock" }).click();
  await expectScreen(page, "No machine has pushed yet");
  expect(await page.evaluate(() => Object.keys(localStorage).filter((k) => k.startsWith("yogurt-cloud")))).toEqual(["yogurt-cloud-browser"]);

  // A second browser unlocks, then the first forgets it and the second lands on Unlock.
  const other = await browser.newContext({ baseURL: info.project.use.baseURL, viewport: page.viewportSize()!, colorScheme: info.project.use.colorScheme });
  const second = await other.newPage();
  await signIn(second);
  await expectScreen(second, "Unlock your meetings");
  await typePin(second, "246810");
  await second.getByRole("button", { name: "Unlock" }).click();
  await expectScreen(second, "No machine has pushed yet");

  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByRole("button", { name: /^Forget/ })).toHaveCount(2);
  await shot(page, info, "s-settings-two-browsers");
  await page.getByRole("button", { name: /^Forget/ }).last().click();
  await expect(page.getByRole("button", { name: /^Forget/ })).toHaveCount(1);
  await second.reload();
  await expectScreen(second, "Unlock your meetings");
  await other.close();

  // Change the PIN. This browser unlocks again with the new one and stays remembered.
  await page.getByRole("button", { name: "Change PIN" }).click();
  await expectScreen(page, "Change PIN");
  await expect(page.locator('div:has(> input[aria-label="Current PIN"]) i[data-filled]')).toHaveCount(6);
  await page.getByLabel("Current PIN").fill("246810");
  await page.getByRole("button", { name: "4 digits" }).click();
  await page.getByLabel("PIN", { exact: true }).fill("1357");
  await page.getByLabel("Confirm PIN").fill("1357");
  await shot(page, info, "change-pin");
  await page.getByRole("button", { name: "Save PIN" }).click();
  await expect(page.getByRole("status")).toContainText("PIN changed");
  await expect(page.getByRole("button", { name: /^Forget/ })).toHaveCount(1);

  // Five wrong tries lock unlocking.
  await page.getByRole("button", { name: /^Lock/ }).first().click();
  await expectScreen(page, "Unlock your meetings");
  await expect(page.locator("i[data-filled]")).toHaveCount(4);
  await shot(page, info, "i-unlock-four");
  for (let i = 0; i < 5; i++) {
    await typePin(page, "9999");
    await page.getByRole("button", { name: "Unlock" }).click();
    if (i < 4) await expect(page.getByText("Wrong PIN. Try again.")).toBeVisible();
  }
  await expect(page.getByText(/Too many wrong tries. Try again in 15 minutes/)).toBeVisible();
  await expect(page.getByRole("button", { name: "Unlock" })).toBeDisabled();
  await shot(page, info, "j-unlock-locked");

  // Sign out returns to Sign in.
  await page.getByRole("button", { name: "Sign out" }).click();
  await expectScreen(page, "Sign in");
  await shot(page, info, "h-signin-after");
});
