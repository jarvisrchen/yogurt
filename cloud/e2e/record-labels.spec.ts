import { expect, test, type Page } from "@playwright/test";
import { createClient } from "@supabase/supabase-js";
import { FakeDeepgram } from "./fakeDeepgram";
import { API, KEY, OWNER, PASSWORD, onboard, query, reset, shot } from "./helpers";

test.use({
  launchOptions: { args: ["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream"] },
  permissions: ["microphone"],
});

const dg = new FakeDeepgram(5193, "fake-dg-key");
test.beforeAll(async () => {
  await reset();
  await dg.start();
});
test.afterAll(() => dg.stop());

const MBP = "33333333-3333-4333-8333-333333333333";
const MINI = "44444444-4444-4444-8444-444444444444";
const OLD = "55555555-5555-4555-8555-555555555555";

/** Two Macs whose labels overlap by name in another case, an archived label that a recording is not offered, and a Mac gone quiet. */
async function seedMacs() {
  const sb = createClient(API, KEY);
  expect((await sb.auth.signInWithPassword({ email: OWNER, password: PASSWORD })).error).toBeNull();
  expect((await sb.from("yogurt_devices").insert([{ id: MBP, name: "MacBook Pro" }, { id: MINI, name: "Mac mini" }, { id: OLD, name: "Old laptop" }])).error).toBeNull();
  // As their inbox checks would. The old laptop has not checked in for a week.
  await query("update yogurt_devices set seen_at = now() - (case id when $1::uuid then interval '0' when $2::uuid then interval '1 hour' else interval '8 days' end)", [MBP, MINI]);
  const push = async (device: string, labels: object[], extra: object = {}) => {
    const { data: pushId } = await sb.rpc("yogurt_start_push", { device_id: device });
    const start = Date.now() - 86_400_000;
    const meeting = { id: "m", title: "Site walk", started_at: start, ended_at: start + 1_800_000, notes_md: "", enriched_md: null, transcript_json: "[]", starred: false, labels, ...extra };
    expect((await sb.from("yogurt_versions").insert({ meeting_id: `${device.slice(0, 4)}-${pushId}`, push_id: pushId, device_id: device, body: { v: 1, meeting } })).error).toBeNull();
  };
  await push(MBP, [{ id: "l-build", name: "Build", color: "matcha" }, { id: "l-clients", name: "Clients", color: "honey" }]);
  await push(MBP, [{ id: "l-old", name: "Old clients", color: "slate" }], { archived_at: new Date().toISOString() });
  await push(MINI, [{ id: "l-build-mini", name: "build", color: "straw" }, { id: "l-ops", name: "Ops", color: "blue" }]);
}

const pushed = async () =>
  (await query<{ body: { meeting: { labels: unknown; for_device: string | null } } }>("select v.body from yogurt_versions v join yogurt_devices d on d.id = v.device_id where d.kind = 'phone' order by v.push_id desc limit 1"))[0]?.body.meeting;
const labelButton = (page: Page) => page.getByRole("button", { name: "Label", exact: true });
const savesTo = (page: Page) => page.getByRole("group", { name: "Saves to" });

test("a recording is saved to a chosen Mac and takes that Mac's labels and new ones, before and while it records", async ({ page }, info) => {
  await onboard(page);
  await seedMacs();
  await page.reload();
  await page.getByRole("button", { name: "Record" }).click();
  await expect(page.getByRole("heading", { name: "Record the room" })).toBeVisible();

  // The Mac seen last is picked, a Mac quiet for a week is not offered, and Label lists the picked Mac's labels without the archive's.
  await expect(savesTo(page).getByRole("button")).toHaveText(["MacBook Pro", "Mac mini"]);
  await expect(savesTo(page).getByRole("button", { name: "MacBook Pro" })).toHaveAttribute("aria-pressed", "true");
  await labelButton(page).click();
  const list = page.getByRole("listbox", { name: "Labels" });
  await expect(list.getByRole("option")).toHaveText(["Build", "Clients"]);
  await list.getByRole("option", { name: "Build" }).click();
  await page.getByLabel("Search or create label").fill("Site visit");
  await expect(list.getByRole("option")).toHaveCount(0);
  await page.getByLabel("Search or create label").press("Enter");
  await expect(list.getByRole("option", { name: "Site visit" })).toHaveAttribute("aria-selected", "true");
  await expect(list.getByRole("option", { name: "Build" })).toHaveAttribute("aria-selected", "true");
  await shot(page, info, "l1-ready-picker", false);
  await page.keyboard.press("Escape");
  await expect(list).toBeHidden();

  // Another Mac lists its own labels beside the ones already picked, which it finds by name or creates.
  await savesTo(page).getByRole("button", { name: "Mac mini" }).click();
  await labelButton(page).click();
  await expect(list.getByRole("option")).toHaveText(["Build", "Ops", "Site visit"]);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "Remove Build label" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Remove Site visit label" })).toBeVisible();
  await shot(page, info, "l2-ready-labels", false);

  // The first push carries the Mac and the labels by name, the shape a Mac reads when it takes the meeting.
  await page.getByRole("button", { name: "Start recording" }).click();
  await expect(page.getByRole("button", { name: "End meeting" })).toBeVisible();
  await expect.poll(pushed).toEqual(
    expect.objectContaining({
      for_device: MINI,
      labels: [
        { id: "build", name: "Build", color: "matcha" },
        { id: "site visit", name: "Site visit", color: "" },
      ],
    }),
  );

  // While it records, the Mac changes, a label comes off, and Enter on a name the list has picks that label.
  await expect(savesTo(page).getByRole("button", { name: "Mac mini" })).toHaveAttribute("aria-pressed", "true");
  await savesTo(page).getByRole("button", { name: "MacBook Pro" }).click();
  await page.getByRole("button", { name: "Remove Site visit label" }).click();
  await labelButton(page).click();
  await page.getByLabel("Search or create label").fill("clients");
  await page.getByLabel("Search or create label").press("Enter");
  await expect(list.getByRole("option", { name: "Clients" })).toHaveAttribute("aria-selected", "true");
  await shot(page, info, "l3-recording-picker", false);
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", { name: "Remove Clients label" })).toBeVisible();
  await shot(page, info, "l4-recording-labels", false);
  await expect.poll(pushed, { timeout: 45_000 }).toEqual(
    expect.objectContaining({
      for_device: MBP,
      labels: [
        { id: "build", name: "Build", color: "matcha" },
        { id: "clients", name: "Clients", color: "honey" },
      ],
    }),
  );
});
