import { expect, test, type Page } from "@playwright/test";
import { createClient } from "@supabase/supabase-js";
import { API, KEY, OWNER, PASSWORD, onboard, reset, shot } from "./helpers";

test.beforeAll(reset);

const MBP = "33333333-3333-4333-8333-333333333333";
const MINI = "44444444-4444-4444-8444-444444444444";
const L = {
  oneOnOne: { id: "l-1on1", name: "1:1s", color: "blue" },
  board: { id: "l-board", name: "Board", color: "slate" },
  clients: { id: "l-clients", name: "Clients", color: "honey" },
  hiring: { id: "l-hiring", name: "Hiring", color: "lilac" },
  investors: { id: "l-investors", name: "Investors", color: "straw" },
  planning: { id: "l-planning", name: "Planning", color: "matcha" },
  product: { id: "l-product", name: "Product", color: "blue" },
  recruiting: { id: "l-recruiting", name: "Recruiting", color: "lilac" },
  oldClients: { id: "l-old-clients", name: "Old clients", color: "slate" },
};

const at = (daysAgo: number, hour: number, minute: number) => {
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  d.setHours(hour, minute, 0, 0);
  return d.getTime();
};

/** The mocks' two Macs: MacBook Pro with eight labels, two starred meetings, and two archived ones, and a Mac mini with one label of its own. */
async function seedMacs() {
  const sb = createClient(API, KEY);
  expect((await sb.auth.signInWithPassword({ email: OWNER, password: PASSWORD })).error).toBeNull();
  expect((await sb.from("yogurt_devices").insert([{ id: MBP, name: "MacBook Pro" }, { id: MINI, name: "Mac mini" }])).error).toBeNull();
  const push = async (device: string, meetings: [string, number, number, { starred?: boolean; labels?: object[]; archived_at?: string }?][]) => {
    const { data: pushId } = await sb.rpc("yogurt_start_push", { device_id: device });
    const rows = meetings.map(([title, start, minutes, extra = {}], i) => ({
      meeting_id: `${device.slice(0, 4)}-${i}`,
      push_id: pushId,
      device_id: device,
      body: { v: 1, meeting: { id: `m${i}`, title, started_at: start, ended_at: start + minutes * 60_000, notes_md: `Notes on ${title}.`, enriched_md: null, transcript_json: "[]", starred: false, labels: [], ...extra } },
    }));
    expect((await sb.from("yogurt_versions").insert(rows)).error).toBeNull();
  };
  await push(MBP, [
    ["Weekly 1:1", at(0, 14, 30), 28, { starred: true, labels: [L.oneOnOne] }],
    ["Q4 planning sync", at(0, 10, 0), 52, { labels: [L.planning, L.product, L.investors] }],
    ["Customer call - Acme", at(1, 16, 0), 41, { starred: true, labels: [L.clients] }],
    ["Hiring loop debrief", at(5, 15, 15), 45, { labels: [L.hiring, L.recruiting] }],
    ["Design review", at(5, 11, 30), 60, { labels: [L.product, L.board] }],
    ["Renewal check-in - Acme", at(12, 14, 0), 30, { labels: [L.clients] }],
    ["Kickoff - Globex", at(12, 10, 30), 48, { labels: [L.clients] }],
    ["Quarterly review - Initech", at(26, 15, 0), 55, { labels: [L.clients] }],
    ["Customer call - Globex", at(34, 13, 30), 37, { labels: [L.clients] }],
    ["Kickoff - Hooli", at(400, 10, 0), 50, { labels: [L.clients, L.oldClients], archived_at: new Date().toISOString() }],
    ["Vendor sync", at(410, 9, 0), 25, { labels: [L.oldClients], archived_at: new Date().toISOString() }],
  ]);
  await push(MINI, [
    ["Standup", at(0, 9, 15), 12, { starred: true, labels: [{ id: "l-ops", name: "Ops", color: "matcha" }] }],
    ["Budget review", at(1, 13, 0), 35],
  ]);
  expect((await sb.rpc("yogurt_search_catch_up")).error).toBeNull();
  await sb.auth.signOut();
}

const range = (page: Page) => page.getByTestId("pager-range");
const filters = (page: Page) => page.getByRole("group", { name: "Filter" });
const filter = (page: Page, name: string) => filters(page).getByRole("button", { name, exact: true });
const device = (page: Page, name: string) => page.getByRole("group", { name: "Device" }).getByRole("button", { name });
const labelsButton = (page: Page) => filters(page).getByRole("button", { name: /^Label/ });
const list = (page: Page) => page.getByRole("listbox", { name: "Labels" });
const row = (page: Page, title: string) => page.getByRole("button", { name: new RegExp(`^${title}`) });

test("Starred works on every Mac, and a picked Mac adds Archive and its labels and drops the device tag", async ({ page }, info) => {
  await onboard(page);
  await seedMacs();
  await page.reload();

  // All devices: All and Starred only, and each row names its Mac.
  await expect(row(page, "Weekly 1:1")).toBeVisible();
  await expect(filters(page).getByRole("button")).toHaveText(["All", "Starred"]);
  await expect(row(page, "Weekly 1:1").getByText("MacBook Pro")).toBeVisible();
  await expect(row(page, "Standup").getByText("Mac mini")).toBeVisible();
  await shot(page, info, "f-all-devices", false);

  // Starred lists the starred meetings of every Mac.
  await filter(page, "Starred").click();
  await expect(range(page)).toHaveText("1–3 of 3");
  await expect(row(page, "Standup").getByText("Mac mini")).toBeVisible();
  await expect(row(page, "Customer call - Acme").getByText("MacBook Pro")).toBeVisible();
  await expect(row(page, "Q4 planning sync")).toHaveCount(0);
  await page.mouse.move(0, 0);
  await shot(page, info, "f-all-starred", false);

  // One Mac: Archive joins the pills, the Labels button sits at the end, the rows lose their tag, and Starred stays picked.
  await device(page, "MacBook Pro").click();
  await expect(filters(page).getByRole("button")).toHaveText(["All", "Starred", "Archive", "Labels"]);
  await expect(filter(page, "Starred")).toHaveAttribute("aria-pressed", "true");
  await expect(range(page)).toHaveText("1–2 of 2");
  await expect(row(page, "Weekly 1:1").getByText("MacBook Pro")).toHaveCount(0);
  await expect(row(page, "Customer call - Acme")).toBeVisible();
  await expect(row(page, "Q4 planning sync")).toHaveCount(0);

  // The list holds the Mac's labels by name, and closes on Escape and on a click outside it.
  await expect(labelsButton(page)).toHaveAttribute("aria-expanded", "false");
  await labelsButton(page).click();
  await expect(labelsButton(page)).toHaveAttribute("aria-expanded", "true");
  await expect(list(page).getByRole("option")).toHaveText(["All labels", "1:1s", "Board", "Clients", "Hiring", "Investors", "Planning", "Product", "Recruiting"]);
  const box = (await list(page).boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(0);
  expect(box.x + box.width).toBeLessThanOrEqual(page.viewportSize()!.width);
  await page.keyboard.press("Escape");
  await expect(list(page)).toHaveCount(0);
  await expect(labelsButton(page)).toBeFocused();
  await labelsButton(page).click();
  await page.mouse.click(5, 5);
  await expect(list(page)).toHaveCount(0);

  // Picking a label closes the list, shows it on the button in the selected style, and narrows the pill: one starred Clients meeting.
  await labelsButton(page).click();
  await list(page).getByRole("option", { name: "Clients", exact: true }).click();
  await expect(list(page)).toHaveCount(0);
  await expect(labelsButton(page)).toHaveAccessibleName("Label Clients");
  await expect(labelsButton(page)).toHaveClass(/bg-blue/);
  await expect(filter(page, "Starred")).toHaveAttribute("aria-pressed", "true");
  await expect(range(page)).toHaveText("1–1 of 1");
  await expect(row(page, "Customer call - Acme")).toBeVisible();

  // All shares Starred's labels, so the label stays.
  await filter(page, "All").click();
  await expect(labelsButton(page)).toHaveAccessibleName("Label Clients");
  await expect(range(page)).toHaveText("1–5 of 5");
  await page.mouse.move(0, 0);
  await shot(page, info, "f-clients", false);
  await labelsButton(page).click();
  await expect(list(page).getByRole("option", { name: "Clients", exact: true })).toHaveAttribute("aria-selected", "true");
  await expect(list(page).getByRole("option", { selected: true })).toHaveCount(1);
  await page.mouse.move(0, 0);
  await shot(page, info, "f-list", false);
  await page.keyboard.press("Escape");

  // Search keeps the label: "review" matches three meetings, and one of them is a Clients meeting on this Mac.
  const search = page.getByRole("textbox", { name: "Search notes and transcripts" });
  await search.fill("review");
  await expect(page.getByRole("status")).toHaveText("1 match");
  await expect(row(page, "Quarterly review - Initech")).toBeVisible();

  // Escape with the list open closes only the list and keeps the query, and a pick returns focus to the button.
  await labelsButton(page).click();
  await page.keyboard.press("Escape");
  await expect(list(page)).toHaveCount(0);
  await expect(search).toHaveValue("review");
  await labelsButton(page).click();
  await list(page).getByRole("option", { name: "Board" }).click();
  await expect(labelsButton(page)).toBeFocused();
  await expect(page.getByRole("status")).toHaveText("1 match");
  await expect(row(page, "Design review")).toBeVisible();
  await search.fill("");

  // The Archive has its own labels, so it starts with none, and a label narrows it.
  await filter(page, "Archive").click();
  await expect(labelsButton(page)).toHaveAccessibleName("Labels");
  await expect(range(page)).toHaveText("1–2 of 2");
  await labelsButton(page).click();
  await expect(list(page).getByRole("option")).toHaveText(["All labels", "Clients", "Old clients"]);
  await list(page).getByRole("option", { name: "Clients", exact: true }).click();
  await expect(filter(page, "Archive")).toHaveAttribute("aria-pressed", "true");
  await expect(range(page)).toHaveText("1–1 of 1");
  await expect(row(page, "Kickoff - Hooli")).toBeVisible();
  await expect(row(page, "Customer call - Acme")).toHaveCount(0);
  await page.mouse.move(0, 0);
  await shot(page, info, "f-archive-clients", false);
  await search.fill("vendor");
  await expect(page.getByRole("status")).toHaveText("No meetings match vendor.");
  await search.fill("");

  // All labels clears the label, and leaving the Archive starts the label over.
  await labelsButton(page).click();
  await list(page).getByRole("option", { name: "All labels" }).click();
  await expect(labelsButton(page)).toHaveAccessibleName("Labels");
  await expect(range(page)).toHaveText("1–2 of 2");
  await labelsButton(page).click();
  await list(page).getByRole("option", { name: "Old clients" }).click();
  await expect(range(page)).toHaveText("1–2 of 2");
  await filter(page, "All").click();
  await expect(labelsButton(page)).toHaveAccessibleName("Labels");
  await expect(range(page)).toHaveText("1–9 of 9");

  // Another Mac starts a label over at All with its own labels, and All devices drops Archive and Labels again.
  await device(page, "Mac mini").click();
  await expect(filter(page, "All")).toHaveAttribute("aria-pressed", "true");
  await expect(labelsButton(page)).toHaveAccessibleName("Labels");
  await expect(range(page)).toHaveText("1–2 of 2");
  await labelsButton(page).click();
  await expect(list(page).getByRole("option")).toHaveText(["All labels", "Ops"]);
  await list(page).getByRole("option", { name: "Ops" }).click();
  await expect(range(page)).toHaveText("1–1 of 1");
  await device(page, "All devices").click();
  await expect(filters(page).getByRole("button")).toHaveText(["All", "Starred"]);
  await expect(filter(page, "All")).toHaveAttribute("aria-pressed", "true");
  await expect(range(page)).toHaveText("1–11 of 11");
});
