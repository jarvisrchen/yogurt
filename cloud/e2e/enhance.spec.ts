import { readFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { expect, test } from "@playwright/test";
import { createClient } from "@supabase/supabase-js";
import { API, DEVICES, KEY, OWNER, PASSWORD, isPhone, onboard, query, reset, seed, shot } from "./helpers";

// The case crates/yogurt-server/tests/wasm_parity.rs runs through the Mac's /enhance route, with the enriched_md the Mac stored.
const [CASE] = JSON.parse(readFileSync(new URL("../../crates/yogurt-wasm/tests/fixture.json", import.meta.url), "utf8"));
const PHONE = "33333333-3333-4333-8333-333333333333";
const MEETING = "01JPHONEMEETING0000000000A";
const STT = "cloud · nova-3 · phone";
// A Mac's adopted copy of a phone meeting keeps the phone's stt_engine, under the Mac's device and a UUID.
const ADOPTED = "0199c0de-0000-7000-8000-000000000001";

// A stand-in for the LLM provider. It streams the fixture's reply in pieces, holding the second half until the spec lets it go, or fails when told to.
const LLM_PORT = Number(process.env.YOGURT_CLOUD_E2E_LLM_PORT ?? 5192);
let fail = false;
let release: () => void = () => undefined;
let held = new Promise<void>((r) => (release = r));
const asked: { model: string; messages: { role: string; content: string }[] }[] = [];
let llm: Server;

test.describe.configure({ mode: "serial" });
test.beforeAll(async () => {
  llm = createServer(async (req, res) => {
    let body = "";
    for await (const c of req) body += c;
    asked.push(JSON.parse(body));
    if (fail) {
      res.writeHead(500, { "content-type": "application/json" });
      return res.end(JSON.stringify({ error: { message: "the model is overloaded" } }));
    }
    res.writeHead(200, { "content-type": "text/event-stream" });
    const reply: string = CASE.llm_output;
    const pieces = reply.match(/[\s\S]{1,40}/g)!;
    for (const [i, content] of pieces.entries()) {
      if (i === Math.floor(pieces.length / 2)) await held;
      res.write(`data: ${JSON.stringify({ choices: [{ delta: { content }, finish_reason: null }] })}\n\n`);
      await new Promise((r) => setTimeout(r, 20));
    }
    res.end(`data: ${JSON.stringify({ choices: [{ delta: {}, finish_reason: "stop" }] })}\n\ndata: [DONE]\n\n`);
  });
  await new Promise<void>((r) => llm.listen(LLM_PORT, "127.0.0.1", r));
});
test.afterAll(() => new Promise<void>((r) => (llm ? llm.close(() => r()) : r())));

/** A meeting recorded on a phone, pushed the way the record screen pushes it, beside the two laptops of `seed`, and Laptop 1's adopted copy of another. */
async function seedPhone(extra: object = {}) {
  const sb = createClient(API, KEY);
  const { error } = await sb.auth.signInWithPassword({ email: OWNER, password: PASSWORD });
  if (error) throw error;
  const ok = (r: { error: { message: string } | null }) => {
    if (r.error) throw new Error(r.error.message);
  };
  ok(await sb.from("yogurt_devices").insert({ id: PHONE, name: "Phone", kind: "phone" }));
  const push = await sb.rpc("yogurt_start_push", { device_id: PHONE });
  ok(push);
  const start = Date.now() - 30 * 60_000;
  const meeting = { id: MEETING, title: "Pricing sync", started_at: start, ended_at: start + 18 * 60_000, notes_md: CASE.notes_md, enriched_md: null, transcript_json: CASE.transcript_json, starred: false, stt_engine: STT, llm_model: null, template: null, created_at: new Date(start).toISOString(), labels: [], ...extra };
  ok(await sb.from("yogurt_versions").insert({ meeting_id: MEETING, push_id: push.data, device_id: PHONE, body: { v: 1, meeting, chat: [] } }));
  const mac = await sb.rpc("yogurt_start_push", { device_id: DEVICES.a });
  ok(mac);
  const adopted = { ...meeting, id: ADOPTED, title: "Adopted sync" };
  ok(await sb.from("yogurt_versions").insert({ meeting_id: ADOPTED, push_id: mac.data, device_id: DEVICES.a, body: { v: 1, meeting: adopted, chat: [] } }));
  await sb.auth.signOut();
}

const versions = () =>
  query<{ push_id: number; body: { v: number; meeting: Record<string, unknown>; chat: unknown[] } }>(
    "select push_id, body from yogurt_versions where device_id = $1 and meeting_id = $2 order by push_id",
    [PHONE, MEETING],
  );

test("enhance a phone meeting with the Mac's prompt and merge", async ({ page }, info) => {
  const phone = isPhone(page);
  // The phone mocks are drawn at a Pixel 10 Pro XL's 412 px.
  if (phone) await page.setViewportSize({ width: 412, height: 915 });
  const name = (s: string) => (phone ? `enhance-${s}` : `desktop-enhance-${s}`);
  fail = false;
  held = new Promise<void>((r) => (release = r));
  await reset();
  await onboard(page);
  await seed();
  await seedPhone();
  await page.reload();

  // A Mac's meeting is read-only here.
  await page.goto(`/#/m/${DEVICES.b}/m-planning`);
  await expect(page.getByRole("heading", { name: "Q4 planning sync" })).toBeVisible();
  await expect(page.getByRole("button", { name: /enhance/i })).toHaveCount(0);

  // So is a Mac's adopted copy of a phone meeting, which still carries the phone's transcript engine.
  await page.goto(`/#/m/${DEVICES.a}/${ADOPTED}`);
  await expect(page.getByRole("heading", { name: "Adopted sync" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Notes" })).toBeVisible();
  await expect(page.getByRole("button", { name: /enhance/i })).toHaveCount(0);

  await page.goto(`/#/m/${PHONE}/${MEETING}`);
  await expect(page.getByRole("heading", { name: "Pricing sync" })).toBeVisible();
  const enhance = page.getByRole("button", { name: "Enhance", exact: true });
  await expect(enhance).toBeVisible();
  await expect(page.getByRole("tab", { name: "Enhanced" })).toHaveCount(0);
  await shot(page, info, name("before"), false);

  await enhance.click();
  const banner = page.getByTestId("enhancing-banner");
  await expect(banner).toContainText("Weaving your notes into the transcript…");
  await expect(page.getByRole("tab", { name: "Enhanced" })).toHaveAttribute("aria-selected", "true");
  // The reasoning and the template line never show in the preview.
  await expect(page.getByRole("tabpanel")).toContainText("Pricing");
  await expect(page.getByRole("tabpanel")).not.toContainText("template:");
  await expect(page.getByRole("tabpanel")).not.toContainText("pick team-meeting");
  await expect(enhance).toHaveCount(0);
  await shot(page, info, name("streaming"), false);

  release();
  await expect(banner).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Re-enhance" })).toBeVisible();
  const doc = page.getByRole("tabpanel");
  await expect(page.getByRole("tab", { name: "Enhanced" })).toHaveAttribute("aria-selected", "true");
  await expect(doc.locator("[data-ai-grey]").first()).toBeVisible();
  await expect(doc.locator(".transcript-link", { hasText: "↳ 10:19" }).first()).toBeVisible();
  await expect(doc).not.toContainText("USER NOTES");
  await shot(page, info, name("done"), false);

  // The function got the Mac's one user message, and the page pushed exactly the enriched_md the Mac stores, keeping every other field.
  expect(asked.at(-1)!.model).toBe("fake-model");
  expect(asked.at(-1)!.messages.map((m) => m.role)).toEqual(["user"]);
  expect(asked.at(-1)!.messages[0].content).toContain(CASE.transcript_json);
  await expect.poll(async () => (await versions()).length).toBe(2);
  const [first, latest] = await versions();
  expect(latest.body.meeting).toEqual({ ...first.body.meeting, enriched_md: CASE.enriched_md, llm_model: "fake-model", template: CASE.expected_template });
  expect(latest.body.chat).toEqual([]);

  // A transcript link opens the line it cites.
  await doc.locator(".transcript-link", { hasText: "↳ 10:19" }).first().click();
  await expect(page.getByRole("tab", { name: "Transcript" })).toHaveAttribute("aria-selected", "true");
  await expect(page.locator("[data-transcript-ts-sec]", { hasText: "eighteen dollars a month" })).toBeInViewport();

  // A failed Re-enhance keeps the meeting as it was and offers a retry.
  fail = true;
  await page.getByRole("button", { name: "Re-enhance" }).click();
  const error = page.getByTestId("enhancing-banner-error");
  await expect(error).toContainText("Enhance failed.");
  await expect(error).toContainText("the model is overloaded");
  await expect(page.getByRole("tab", { name: "Notes" })).toBeVisible();
  await expect(page.getByRole("tab", { name: "Transcript" })).toBeVisible();
  await page.getByRole("tab", { name: "Enhanced" }).click();
  await expect(page.getByRole("tabpanel").locator("[data-ai-grey]").first()).toBeVisible();
  await shot(page, info, name("error"), false);
  expect(await versions()).toHaveLength(2);

  fail = false;
  await page.getByTestId("enhancing-banner-retry").click();
  await expect(error).toHaveCount(0);
  await expect(page.getByTestId("enhancing-banner")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Re-enhance" })).toBeVisible();
  // The Re-enhance kept the format the first enhance used.
  expect(asked.at(-1)!.messages[0].content).toContain("Note format: Team meeting");
  await expect.poll(async () => (await versions()).length).toBe(3);
});

/** What a Mac's push pass does with an ended phone meeting: it claims the phone's latest version, then pushes its own copy under a UUID. */
async function adoptAsMac(id: string) {
  const sb = createClient(API, KEY);
  const { error } = await sb.auth.signInWithPassword({ email: OWNER, password: PASSWORD });
  if (error) throw error;
  const [latest] = (await versions()).slice(-1);
  const adopt = await sb.rpc("yogurt_adopt", { device_id: PHONE, meeting_id: MEETING, push_id: latest.push_id });
  if (adopt.data !== true) throw new Error(`adopt: ${adopt.error?.message ?? adopt.data}`);
  const push = await sb.rpc("yogurt_start_push", { device_id: DEVICES.a });
  const copy = await sb.from("yogurt_versions").insert({ meeting_id: id, push_id: push.data, device_id: DEVICES.a, body: { ...latest.body, meeting: { ...latest.body.meeting, id } } });
  if (copy.error) throw new Error(copy.error.message);
  await sb.auth.signOut();
}

test("a Mac that adopts the meeting while its Enhance streams keeps it, and the page says the meeting is the Mac's now", async ({ page }, info) => {
  const phone = isPhone(page);
  if (phone) await page.setViewportSize({ width: 412, height: 915 });
  fail = false;
  held = new Promise<void>((r) => (release = r));
  await reset();
  await onboard(page);
  await seed();
  await seedPhone();
  await page.reload();

  await page.goto(`/#/m/${PHONE}/${MEETING}`);
  await page.getByRole("button", { name: "Enhance", exact: true }).click();
  await expect(page.getByTestId("enhancing-banner")).toContainText("Weaving your notes into the transcript…");
  await adoptAsMac("0199c0de-0000-7000-8000-000000000002");
  release();

  await expect(page.getByText("A Mac took this meeting, so it now lists under that Mac.")).toBeVisible();
  await expect(page.getByTestId("enhancing-banner")).toHaveCount(0);
  await expect(page.getByTestId("enhancing-banner-error")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /enhance|retry/i })).toHaveCount(0);
  await shot(page, info, phone ? "enhance-adopted" : "desktop-enhance-adopted", false);
  // The phone's copy stays adopted, so the next pass cannot delete the Mac's copy in its place.
  expect((await versions()).at(-1)!.body).toBeNull();

  if (phone) await page.getByRole("button", { name: "All devices" }).click();
  await expect(page.getByTestId("list-row").filter({ hasText: "Pricing sync" })).toHaveCount(1);
  await expect(page.getByTestId("list-row").filter({ hasText: "Pricing sync" })).toContainText("Laptop 1");
});

test("Enhance on a phone meeting too short to enhance deletes it and goes back to the library, as the Mac does", async ({ page }) => {
  await reset();
  await onboard(page);
  await seed();
  await seedPhone({ title: "Accidental start", notes_md: "", transcript_json: JSON.stringify([{ ts_ms: 0, channel: "them", text: "hello there" }]) });
  await page.reload();
  await page.goto(`/#/m/${PHONE}/${MEETING}`);
  await expect(page.getByRole("heading", { name: "Accidental start" })).toBeVisible();
  const asks = asked.length;
  await page.getByRole("button", { name: "Enhance", exact: true }).click();
  await expect(page.getByText("Meeting too short")).toBeVisible();
  await expect(page.getByText("Meeting too short")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /edis update/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /Accidental start/ })).toHaveCount(0);
  expect(asked.length).toBe(asks);
  expect((await versions()).at(-1)!.body).toBeNull();
});
