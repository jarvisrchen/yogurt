import { expect, test, type Page } from "@playwright/test";
import { createClient } from "@supabase/supabase-js";
import { API, DEVICES, KEY, OWNER, PASSWORD, PIN, expectScreen, isPhone, onboard, query, reset, seed, shot, signIn, typePin } from "./helpers";

const PHONE = "44444444-4444-4444-8444-444444444444";
const PHONE_MEETING = "01JPHONEEDIT00000000000000";

test.describe.configure({ mode: "serial" });
test.beforeAll(reset);

/** The owner's client, signed in as a laptop is: no browser token. */
async function owner() {
  const sb = createClient(API, KEY);
  const { error } = await sb.auth.signInWithPassword({ email: OWNER, password: PASSWORD });
  if (error) throw error;
  return sb;
}

async function seedPhoneMeeting() {
  const sb = await owner();
  const ok = (r: { error: { message: string } | null }) => {
    if (r.error) throw new Error(r.error.message);
  };
  ok(await sb.from("yogurt_devices").insert({ id: PHONE, name: "Pixel", kind: "phone" }));
  const push = await sb.rpc("yogurt_start_push", { device_id: PHONE });
  ok(push);
  const start = Date.now() - 60 * 60_000;
  const meeting = { id: PHONE_MEETING, title: "Untitled meeting", started_at: start, ended_at: start + 5 * 60_000, notes_md: "", enriched_md: null, transcript_json: "[]", starred: false, stt_engine: "cloud · nova-3 · phone", llm_model: null, template: null, created_at: new Date(start).toISOString(), labels: [] };
  ok(await sb.from("yogurt_versions").insert({ meeting_id: PHONE_MEETING, push_id: push.data, device_id: PHONE, body: { v: 1, meeting, chat: [] } }));
  await sb.auth.signOut();
}

/** What a Mac's pass does with a rename: push the renamed meeting, then report the edit made. */
async function macRenames(editId: number, meetingId: string, title: string) {
  const sb = await owner();
  const [{ body }] = await query<{ body: { meeting: Record<string, unknown> } }>(
    "select body from yogurt_versions where device_id = $1 and meeting_id = $2 order by push_id desc limit 1",
    [DEVICES.a, meetingId],
  );
  const push = await sb.rpc("yogurt_start_push", { device_id: DEVICES.a });
  const insert = await sb.from("yogurt_versions").insert({ meeting_id: meetingId, push_id: push.data, device_id: DEVICES.a, body: { ...body, meeting: { ...body.meeting, title } } });
  if (insert.error) throw new Error(insert.error.message);
  const done = await sb.rpc("yogurt_finish_edit", { device_id: DEVICES.a, edit_id: editId, applied: true });
  expect(done.data).toBe(true);
  await sb.auth.signOut();
}

const edits = () => query<{ id: number; meeting_id: string; op: string; value: string; base: string | null; status: string }>("select id, meeting_id, op, value, base, status from yogurt_edits order by id");
const heading = (page: Page, name: string) => page.getByRole("heading", { name, exact: true });
const row = (page: Page, name: RegExp) => page.getByTestId("list-row").filter({ hasText: name });
const backToLibrary = async (page: Page) => {
  if (isPhone(page)) await page.getByRole("button", { name: "All devices" }).first().click();
};

test("a Mac's meeting is renamed, noted, and deleted through that Mac", async ({ page }, info) => {
  await onboard(page);
  await seed();
  await seedPhoneMeeting();
  await page.reload();

  await page.getByRole("button", { name: /edis update \(revised\)/ }).click();
  await expect(heading(page, "edis update (revised)")).toBeVisible();
  await shot(page, info, "edits-actions", false);

  // Rename: the new title shows at once, marked as waiting for the Mac.
  await page.getByRole("button", { name: "Rename" }).click();
  await page.getByLabel("New title").fill("Standup with Edis");
  await shot(page, info, "edits-rename-form", false);
  await page.getByRole("button", { name: "Save title" }).click();
  await expect(heading(page, "Standup with Edis")).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Waiting for Laptop 1 to rename this to “Standup with Edis”." })).toBeVisible();
  const [rename] = await edits();
  expect(rename).toMatchObject({ meeting_id: "m-standup", op: "rename", value: "Standup with Edis", base: "edis update (revised)", status: "pending" });

  // A note shows at the end of the notes, stamped, and Undo takes it back.
  await page.getByRole("button", { name: "Add note" }).click();
  await page.getByLabel("New note").fill("Bring the kombucha");
  await page.getByRole("button", { name: "Add note" }).click();
  await expect(page.getByText("Bring the kombucha")).toBeVisible();
  await expect(page.getByText(/^Added in Yogurt Cloud, /)).toBeVisible();
  await shot(page, info, "edits-waiting", false);
  await page.getByRole("status").filter({ hasText: "add your note" }).getByRole("button", { name: "Undo" }).click();
  await expect(page.getByText("Bring the kombucha")).toHaveCount(0);
  expect((await edits()).map((e) => e.status)).toEqual(["pending", "cancelled"]);

  // The Mac makes the rename on a push while the page still shows it waiting, so a late Undo says it is too late.
  await macRenames(rename.id, "m-standup", "Standup with Edis");
  await page.getByRole("status").filter({ hasText: "rename this to" }).getByRole("button", { name: "Undo" }).click();
  await expect(page.getByText("Laptop 1 already made this change, so it can no longer be undone.")).toBeVisible();
  await expect(page.getByText(/^Waiting for/)).toHaveCount(0);
  await expect(heading(page, "Standup with Edis")).toBeVisible();
  expect((await edits())[0].status).not.toBe("pending");
  await page.reload();
  await expect(heading(page, "Standup with Edis")).toBeVisible();
  await expect(page.getByText(/^Waiting for/)).toHaveCount(0);

  // Delete waits for the Mac too, and the library stops listing the meeting meanwhile.
  await page.getByRole("button", { name: "Delete" }).click();
  await expect(page.getByText("Delete this meeting? Laptop 1 deletes it, and your Cloud keeps it in History.")).toBeVisible();
  await shot(page, info, "edits-delete-confirm", false);
  await page.getByRole("button", { name: "Delete" }).click();
  await expect(page.getByRole("status").filter({ hasText: "Waiting for Laptop 1 to delete this meeting." })).toBeVisible();
  await backToLibrary(page);
  await expect(row(page, /Q4 planning sync/)).toBeVisible();
  await expect(row(page, /Standup with Edis/)).toHaveCount(0);
  expect((await edits()).at(-1)).toMatchObject({ op: "delete", status: "pending" });
});

test("a phone's own meeting changes at once, without a Mac", async ({ page }, info) => {
  await signIn(page);
  await expectScreen(page, "Unlock your meetings");
  await typePin(page, PIN);
  await page.getByRole("button", { name: "Unlock" }).click();
  await page.getByRole("button", { name: /Untitled meeting/ }).click();
  await page.getByRole("button", { name: "Rename" }).click();
  await page.getByLabel("New title").fill("Coffee with Maya");
  await page.getByLabel("New title").press("Enter");
  await expect(heading(page, "Coffee with Maya")).toBeVisible();
  await expect(page.getByText(/^Waiting for/)).toHaveCount(0);

  await page.getByRole("button", { name: "Add note" }).click();
  await page.getByLabel("New note").fill("She recommended the Lisbon talk");
  await page.getByRole("button", { name: "Add note" }).click();
  await expect(page.getByText("She recommended the Lisbon talk")).toBeVisible();
  await shot(page, info, "edits-phone-meeting", false);

  const pushed = await query<{ title: string; notes: string }>(
    "select body #>> '{meeting,title}' as title, body #>> '{meeting,notes_md}' as notes from yogurt_versions where device_id = $1 and meeting_id = $2 order by push_id desc limit 1",
    [PHONE, PHONE_MEETING],
  );
  expect(pushed[0].title).toBe("Coffee with Maya");
  expect(pushed[0].notes).toMatch(/^\*\*Added in Yogurt Cloud, .+\*\*\n\nShe recommended the Lisbon talk$/);
  expect(await query("select 1 from yogurt_edits where device_id = $1", [PHONE])).toHaveLength(0);

  await page.getByRole("button", { name: "Delete" }).click();
  await expect(page.getByText("Delete this meeting? Your Cloud keeps it in History.")).toBeVisible();
  await page.getByRole("button", { name: "Delete" }).click();
  if (isPhone(page)) await expect(page.getByTestId("list-row").first()).toBeVisible();
  await expect(row(page, /Coffee with Maya/)).toHaveCount(0);
  const [last] = await query<{ body: unknown }>("select body from yogurt_versions where device_id = $1 and meeting_id = $2 order by push_id desc limit 1", [PHONE, PHONE_MEETING]);
  expect(last.body).toBeNull();
});
