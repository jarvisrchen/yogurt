import { readFileSync } from "node:fs";
import { expect, type Page, type TestInfo } from "@playwright/test";
import pg from "pg";

const DB = process.env.YOGURT_CLOUD_E2E_DB ?? "postgresql://postgres:postgres@127.0.0.1:56432/postgres";
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
  for (let i = 0; i < 40; i++) {
    const list = await (await fetch(`${MAIL}/api/v1/messages`)).json();
    const id = list.messages?.[0]?.ID;
    if (id) {
      const msg = await (await fetch(`${MAIL}/api/v1/message/${id}`)).json();
      const m = /href="([^"]*\/auth\/v1\/verify[^"]*)"/.exec(msg.HTML ?? "");
      if (m) return m[1].replaceAll("&amp;", "&");
    }
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error("no confirmation email arrived");
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
