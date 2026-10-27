import { readFileSync } from "node:fs";
import { expect, type Page, type TestInfo } from "@playwright/test";
import { createClient } from "@supabase/supabase-js";
import pg from "pg";

const DB = process.env.YOGURT_CLOUD_E2E_DB ?? "postgresql://postgres:postgres@127.0.0.1:56432/postgres";
export const API = process.env.YOGURT_CLOUD_E2E_API ?? "http://127.0.0.1:56431";
export const KEY = process.env.YOGURT_CLOUD_E2E_KEY ?? "sb_publishable_ACJWlzQHlZjBrEguHvfOxg_3BJgxAaH";
const MAIL = process.env.YOGURT_CLOUD_E2E_MAIL ?? "http://127.0.0.1:56434";
export const OWNER = "owner@yogurt-cloud.test";
export const PASSWORD = "correct horse battery";
const sql = (p: string) => readFileSync(new URL(`../../crates/yogurt-cloud/${p}`, import.meta.url), "utf8");

/** Fresh owner, no users, no passcode, empty mailbox. */
export async function reset() {
  const c = new pg.Client({ connectionString: DB });
  await c.connect();
  try {
    await c.query("drop table if exists yogurt_versions, yogurt_pushes, yogurt_devices, yogurt_browsers, yogurt_owner cascade; delete from auth.users;");
    await c.query(sql("schema.sql"));
    await c.query(sql("owner.sql"), [OWNER]);
  } finally {
    await c.end();
  }
  await fetch(`${MAIL}/api/v1/messages`, { method: "DELETE" });
}

/** The confirmation link from the newest message in the local mail catcher. */
export async function confirmationLink(): Promise<string> {
  let link = "";
  await expect
    .poll(async () => {
      const id = (await (await fetch(`${MAIL}/api/v1/messages`)).json()).messages?.[0]?.ID;
      const html: string = id ? ((await (await fetch(`${MAIL}/api/v1/message/${id}`)).json()).HTML ?? "") : "";
      link = /href="([^"]*\/auth\/v1\/verify[^"]*)"/.exec(html)?.[1].replaceAll("&amp;", "&") ?? "";
      return link;
    }, { message: "no confirmation email arrived", timeout: 10_000 })
    .not.toBe("");
  return link;
}

export const isPhone = (page: Page) => (page.viewportSize()?.width ?? 1000) <= 720;

/** Types a PIN the way the screen takes it: the on-screen keypad on a phone, the keyboard elsewhere. */
export async function typePin(page: Page, pin: string, label = "PIN") {
  if (isPhone(page) && (await page.getByRole("button", { name: "Delete" }).isVisible())) {
    for (const d of pin) await page.getByRole("button", { name: new RegExp(`^${d}`) }).click();
  } else {
    await page.getByLabel(label, { exact: true }).fill(pin);
  }
}

export async function shot(page: Page, info: TestInfo, name: string) {
  await page.waitForTimeout(150);
  await page.screenshot({ path: `e2e/screenshots/${info.project.name}/${name}.png`, fullPage: true });
}

export async function signIn(page: Page) {
  await page.goto("/");
  await page.getByLabel("Email").fill(OWNER);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Sign in" }).click();
}

export const expectScreen = (page: Page, heading: string | RegExp) => expect(page.getByRole("heading", { name: heading })).toBeVisible();

export const PIN = "246810";

/** Create the login, confirm it from the mail catcher, and choose the PIN. Lands on the library. */
export async function onboard(page: Page) {
  await page.goto("/");
  await page.getByRole("button", { name: "Create your Cloud login" }).click();
  await page.getByLabel("Email").fill(OWNER);
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Create login" }).click();
  await expectScreen(page, "Check your email");
  await page.goto(await confirmationLink());
  await expectScreen(page, /Choose a 4 or 6 digit PIN/);
  await page.getByLabel("PIN", { exact: true }).fill(PIN);
  await page.getByLabel("Confirm PIN").fill(PIN);
  await page.getByRole("button", { name: "Save PIN" }).click();
  await expectScreen(page, "No machine has pushed yet");
}

const at = (daysAgo: number, hour: number, minute: number) => {
  const d = new Date();
  d.setDate(d.getDate() - daysAgo);
  d.setHours(hour, minute, 0, 0);
  return d.getTime();
};
const meeting = (id: string, title: string, start: number, minutes: number, over: Record<string, unknown> = {}) => ({
  v: 1,
  meeting: { id, title, started_at: start, ended_at: start + minutes * 60_000, notes_md: "", enriched_md: null, transcript_json: "[]", ...over },
});

export const LATE = { ts_ms: 600_000, text: "Late in the call: sign the vendor contract on Friday." };
/** The three lines the specs read near the top, then 68 more every 10 seconds, so the transcript runs 12 minutes and needs scrolling. */
const SEGMENTS = JSON.stringify([
  { ts_ms: 12_000, channel: "them", text: "We should lock the pricing page before the freeze." },
  { ts_ms: 41_000, channel: "me", text: "Agreed. Can you own the migration plan?" },
  { ts_ms: 45_000, channel: "them", text: "Also, who is bringing the kombucha to the offsite?" },
  ...Array.from({ length: 68 }, (_, i) => {
    const ts_ms = 50_000 + i * 10_000;
    return ts_ms === LATE.ts_ms ? { ...LATE, channel: "me" } : { ts_ms, channel: i % 2 ? "me" : "them", text: `Filler line ${i + 3} about the roadmap.` };
  }),
]);

export const DEVICES = { a: "11111111-1111-4111-8111-111111111111", b: "22222222-2222-4222-8222-222222222222" };

/**
 * Two laptops and four pushes, written the way a laptop writes them: the owner inserts devices and versions and starts each push through its RPC.
 * Push 1 (Laptop 1) and 2 (Laptop 2) add four meetings, push 3 (Laptop 1) edits one and removes one, push 4 (Laptop 2) removes one.
 * Then search catches up, as a laptop does after its push.
 */
export async function seed() {
  const sb = createClient(API, KEY);
  const { error: authError } = await sb.auth.signInWithPassword({ email: OWNER, password: PASSWORD });
  if (authError) throw authError;
  const ok = (r: { error: { message: string } | null }) => {
    if (r.error) throw new Error(r.error.message);
  };
  ok(await sb.from("yogurt_devices").insert([{ id: DEVICES.a, name: "Laptop 1" }, { id: DEVICES.b, name: "Laptop 2" }]));
  const push = async (device: string, versions: { meeting_id: string; body: unknown }[]) => {
    const { data, error } = await sb.rpc("yogurt_start_push", { device_id: device });
    if (error) throw new Error(error.message);
    ok(await sb.from("yogurt_versions").insert(versions.map((v) => ({ ...v, push_id: data, device_id: device }))));
  };
  const standup = meeting("m-standup", "edis update", at(0, 14, 59), 11, { notes_md: "Ship the **edis** dashboard." });
  const vendor = meeting("m-vendor", "Vendor review", at(2, 10, 37), 41, { notes_md: "Ask for the SOC 2 report." });
  const planning = meeting("m-planning", "Q4 planning sync", at(2, 13, 42), 22, {
    notes_md: "## Decisions\n\n- Ship the pricing page before the holiday freeze",
    enriched_md: `## Decisions\n\n- Ship the pricing page before the holiday freeze\n- <span data-ai-grey="">Steven owns the migration plan, due Nov 3</span>\n\n## Open items\n\n- <span data-ai-grey="">Confirm the vendor\'s SOC 2 report</span> <span data-transcript-link data-ts="${LATE.ts_ms / 1000}">↳ 10:00</span>`,
    transcript_json: SEGMENTS,
  });
  const steven = meeting("m-steven", "1:1 with Steven", at(10, 11, 59), 30, { notes_md: "Career path." });
  await push(DEVICES.a, [{ meeting_id: "m-standup", body: standup }, { meeting_id: "m-vendor", body: vendor }]);
  await push(DEVICES.b, [{ meeting_id: "m-planning", body: planning }, { meeting_id: "m-steven", body: steven }]);
  await push(DEVICES.a, [{ meeting_id: "m-standup", body: { ...standup, meeting: { ...standup.meeting, title: "edis update (revised)" } } }, { meeting_id: "m-vendor", body: null }]);
  await push(DEVICES.b, [{ meeting_id: "m-steven", body: null }]);
  ok(await sb.rpc("yogurt_search_catch_up"));
  await sb.auth.signOut();
}
