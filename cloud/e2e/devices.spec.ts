import { expect, test, type Page } from "@playwright/test";
import { createClient } from "@supabase/supabase-js";
import { API, KEY, OWNER, PASSWORD, onboard, reset, shot } from "./helpers";

test.beforeAll(reset);

const MACS = [
  { id: "55555555-5555-4555-8555-555555555551", name: "MacBook Pro" },
  { id: "55555555-5555-4555-8555-555555555552", name: "Mac mini" },
  { id: "55555555-5555-4555-8555-555555555553", name: "iMac" },
  { id: "55555555-5555-4555-8555-555555555554", name: "Work laptop" },
];

/** Four Macs with one meeting each, so every row carries its device tag. */
async function seedMacs() {
  const sb = createClient(API, KEY);
  expect((await sb.auth.signInWithPassword({ email: OWNER, password: PASSWORD })).error).toBeNull();
  for (const [i, mac] of MACS.entries()) {
    expect((await sb.from("yogurt_devices").insert({ id: mac.id, name: mac.name })).error).toBeNull();
    const { data: pushId } = await sb.rpc("yogurt_start_push", { device_id: mac.id });
    const start = Date.now() - (i + 1) * 3_600_000;
    const body = { v: 1, meeting: { id: "m", title: `Standup on ${mac.name}`, started_at: start, ended_at: start + 900_000, notes_md: "", enriched_md: null, transcript_json: "[]" } };
    expect((await sb.from("yogurt_versions").insert({ meeting_id: `m-${i}`, push_id: pushId, device_id: mac.id, body })).error).toBeNull();
  }
  await sb.auth.signOut();
}

const pills = (page: Page) => page.getByRole("group", { name: "Device" }).getByRole("button");
const pill = (page: Page, name: string) => page.getByRole("group", { name: "Device" }).getByRole("button", { name, exact: true });
const tag = (page: Page, mac: string) => page.getByRole("button", { name: new RegExp(`^Standup on ${mac}`) }).getByText(mac, { exact: true });
const dialog = (page: Page) => page.getByRole("dialog");

test("a device keeps the color picked for it and the order set for it", async ({ page }, info) => {
  await onboard(page);
  await seedMacs();
  await page.reload();

  // Without a stored color a device cycles blue, honey, slate by connect order, the same in its pill dot and its tag.
  await expect(pills(page)).toHaveText(["All devices", "MacBook Pro", "Mac mini", "iMac", "Work laptop"]);
  await expect(tag(page, "MacBook Pro")).toHaveAttribute("style", /--color-blue\)/);
  await expect(tag(page, "Mac mini")).toHaveAttribute("style", /--color-honey\)/);
  await expect(tag(page, "iMac")).toHaveAttribute("style", /--color-slate\)/);
  await expect(tag(page, "Work laptop")).toHaveAttribute("style", /--color-blue\)/);

  // A picked device fills with its color, and tapping it again opens its settings.
  await pill(page, "Mac mini").click();
  await expect(pill(page, "Mac mini")).toHaveAttribute("style", /--color-honeysoft/);
  await expect(dialog(page)).toHaveCount(0);
  await shot(page, info, "d-picked", false);
  await pill(page, "Mac mini").click();
  await expect(dialog(page)).toBeVisible();
  await expect(dialog(page).getByRole("heading")).toHaveText("Mac mini");
  await expect(dialog(page).getByRole("radio", { name: "honey" })).toHaveAttribute("aria-checked", "true");
  await shot(page, info, "d-color", false);

  // A swatch saves at once, and a typed hex saves on Enter.
  await dialog(page).getByRole("radio", { name: "lilac" }).click();
  await expect(pill(page, "Mac mini")).toHaveAttribute("style", /--color-lilac/);
  await dialog(page).getByLabel("Custom color hex").fill("#2E8B57");
  await dialog(page).getByLabel("Custom color hex").press("Enter");
  await expect(pill(page, "Mac mini")).toHaveAttribute("style", /rgb\(46, 139, 87\)/);
  await expect(dialog(page).getByRole("radio", { name: "lilac" })).toHaveAttribute("aria-checked", "false");
  await shot(page, info, "d-custom", false);

  // Escape closes the dialog and leaves the device picked.
  await page.keyboard.press("Escape");
  await expect(dialog(page)).toHaveCount(0);
  await expect(pill(page, "Mac mini")).toHaveAttribute("aria-pressed", "true");

  // Reorder: the arrow keys move a focused handle, and a drag moves a row.
  await pill(page, "Mac mini").click();
  await dialog(page).getByRole("button", { name: "Reorder devices…" }).click();
  const rows = dialog(page).getByRole("list", { name: "Devices" }).getByRole("listitem");
  await expect(rows).toHaveText(["MacBook Pro", "Mac mini", "iMac", "Work laptop"]);
  await shot(page, info, "d-reorder", false);
  await dialog(page).getByRole("button", { name: "Move Work laptop" }).focus();
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("ArrowUp");
  await expect(rows).toHaveText(["MacBook Pro", "Work laptop", "Mac mini", "iMac"]);
  await expect(dialog(page).getByRole("button", { name: "Move Work laptop" })).toBeFocused();
  await expect(pills(page)).toHaveText(["All devices", "MacBook Pro", "Work laptop", "Mac mini", "iMac"]);

  const step = (await rows.first().boundingBox())!.height + 6;
  const drag = async (name: string, by: number) => {
    const handle = (await dialog(page).getByRole("button", { name: `Move ${name}` }).boundingBox())!;
    const x = handle.x + handle.width / 2;
    const y = handle.y + handle.height / 2;
    await page.mouse.move(x, y);
    await page.mouse.down();
    for (let i = 1; i <= 6; i++) await page.mouse.move(x, y + (i * by * step) / 6);
    await page.mouse.up();
  };
  await drag("iMac", -3);
  await expect(rows).toHaveText(["iMac", "MacBook Pro", "Work laptop", "Mac mini"]);
  await expect(pills(page)).toHaveText(["All devices", "iMac", "MacBook Pro", "Work laptop", "Mac mini"]);
  // Down moves the dragged row's own node, which must not end the drag.
  await drag("MacBook Pro", 2);
  await expect(rows).toHaveText(["iMac", "Work laptop", "Mac mini", "MacBook Pro"]);
  await expect(pills(page)).toHaveText(["All devices", "iMac", "Work laptop", "Mac mini", "MacBook Pro"]);
  await dialog(page).getByRole("button", { name: "Done" }).click();
  await expect(dialog(page)).toHaveCount(0);

  // Both survive a reload, and moving a device kept its default color.
  await page.reload();
  await expect(pills(page)).toHaveText(["All devices", "iMac", "Work laptop", "Mac mini", "MacBook Pro"]);
  await expect(tag(page, "Mac mini")).toHaveAttribute("style", /rgb\(46, 139, 87\)/);
  await expect(tag(page, "iMac")).toHaveAttribute("style", /--color-slate\)/);
  await expect(tag(page, "Work laptop")).toHaveAttribute("style", /--color-blue\)/);
  await shot(page, info, "d-after", false);
});
