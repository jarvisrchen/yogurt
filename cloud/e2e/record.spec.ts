import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, test, type Page } from "@playwright/test";
import { FakeDeepgram } from "./fakeDeepgram";
import { isPhone, onboard, query, reset, seed, shot } from "./helpers";

// Chrome's fake mic plays this file on a loop. It is spoken, with a pause that ends each line, so real Deepgram transcribes it too.
const dir = fileURLToPath(new URL("./.audio/", import.meta.url));
const WAV = `${dir}room.wav`;
if (!existsSync(WAV)) {
  mkdirSync(dir, { recursive: true });
  execFileSync("say", ["-o", `${dir}room.aiff`, "Okay, next one. Pro pricing. Dana's proposal is eighteen dollars a month and one hundred eighty a year. [[slnc 2000]]"]);
  execFileSync("afconvert", ["-f", "WAVE", "-d", "LEI16@16000", "-c", "1", `${dir}room.aiff`, WAV]);
}

test.use({
  launchOptions: { args: ["--use-fake-ui-for-media-stream", "--use-fake-device-for-media-stream", `--use-file-for-fake-audio-capture=${WAV}`] },
  permissions: ["microphone"],
});

const dg = new FakeDeepgram(5193, "fake-dg-key");
test.describe.configure({ mode: "serial" });
test.beforeAll(async () => {
  await reset();
  await dg.start();
});
test.afterAll(() => dg.stop());

const sheet = (page: Page) => page.getByRole("region", { name: "Live transcript" });
const finals = (page: Page) => sheet(page).locator('[data-final="true"]');
/** The number of the newest final line the fake sent that the sheet shows. The peek shows only two rows. */
const newest = async (page: Page) => Number(/Heard line (\d+)/.exec((await finals(page).last().textContent()) ?? "")?.[1] ?? 0);

type Version = { body: { meeting: { id: string; title: string; ended_at: number | null; notes_md: string; transcript_json: string; stt_engine: string } } };
const latest = async (deviceId: string) =>
  (await query<Version>("select body from yogurt_versions where device_id = $1 order by push_id desc limit 1", [deviceId]))[0]?.body.meeting;

test("record on a phone: live lines, notes, a dropped socket, a killed tab, and End", async ({ page }, info) => {
  test.setTimeout(240_000);
  const phone = isPhone(page);
  // The Pixel 10 Pro XL's CSS viewport.
  if (phone) await page.setViewportSize({ width: 412, height: 915 });
  const name = phone ? "Pixel 10" : "Desk mic";
  await reset();
  await onboard(page);
  await seed();
  await page.reload();
  await expect(page.getByRole("button", { name: /edis update/ })).toBeVisible();
  await shot(page, info, "r1-library-record", false);

  await page.getByRole("button", { name: "Record" }).click();
  await expect(page.getByRole("heading", { name: "Record the room" })).toBeVisible();
  await page.getByLabel("Title").fill("Q4 pricing sync");
  await page.getByLabel("This phone's name").fill(name);
  await shot(page, info, "r2-ready", false);

  const grantsBefore = dg.grants;
  const socketsBefore = dg.sockets.length;
  await page.getByRole("button", { name: "Start recording" }).click();
  await expect(page.getByRole("button", { name: "End meeting" })).toBeVisible();
  await expect(sheet(page).getByText("connected")).toBeVisible();
  await expect(finals(page).first()).toBeVisible({ timeout: 15_000 });

  // The audio is the mic's, through the worklet, at 16 kHz, on a socket opened with a granted token and the Mac's settings.
  const first = dg.sockets[socketsBefore];
  expect(dg.grants).toBe(grantsBefore + 1);
  expect(first.token).toBe(`granted-${dg.grants}`);
  expect(Object.fromEntries(first.query)).toMatchObject({ model: "nova-3", encoding: "linear16", sample_rate: "16000", channels: "1", interim_results: "true", endpointing: "1000" });
  expect(first.loud).toBe(true);
  const elapsed = (Date.now() - first.openedAt) / 1000;
  expect(first.bytes / 32_000).toBeGreaterThan(elapsed - 1.5);
  expect(first.bytes / 32_000).toBeLessThan(elapsed + 1.5);

  // The phone registered as a phone device, under the name typed.
  const [device] = await query<{ id: string; name: string; kind: string }>("select id::text, name, kind from yogurt_devices where kind = 'phone'");
  expect(device).toMatchObject({ name, kind: "phone" });

  const editor = page.locator(".ProseMirror");
  await editor.click();
  await page.keyboard.type("## Pricing\npro tier at $18?\nask finance re annual cap");
  await expect(sheet(page).getByText(/Heard line 2/)).toBeVisible({ timeout: 15_000 });
  await shot(page, info, "r3-recording", false);

  await sheet(page).getByRole("button", { name: "Raise transcript" }).click();
  await expect(sheet(page).getByRole("button", { name: "Lower transcript" })).toBeVisible();
  await expect(finals(page).first()).toBeInViewport();
  await shot(page, info, "r4-sheet-up", false);
  await sheet(page).getByRole("button", { name: "Lower transcript" }).click();
  await expect(sheet(page).getByRole("button", { name: "Raise transcript" })).toBeVisible();

  // A dropped socket: the page holds the audio, takes a fresh token once Deepgram answers again, and replays what it held at once, in order.
  const lineBefore = await newest(page);
  const dropAt = Date.now();
  dg.drop(4_000);
  await expect(sheet(page).getByText("reconnecting")).toBeVisible();
  await expect(page.getByText(/Reconnecting… Holding \d:\d\d of audio/)).toBeVisible();
  await shot(page, info, "r5-reconnecting", false);
  await expect(sheet(page).getByText("connected")).toBeVisible({ timeout: 20_000 });
  const replay = dg.sockets.at(-1)!;
  expect(replay.token).toBe(`granted-${dg.grants}`);
  expect(replay.openedAt - dropAt).toBeGreaterThan(3_500);
  expect(replay.burst / 32_000).toBeGreaterThan(3);
  await expect.poll(() => newest(page), { timeout: 15_000 }).toBeGreaterThan(lineBefore + 1);

  // The 30 second push carries the transcript so far, with ended_at still null.
  await expect.poll(async () => JSON.parse((await latest(device.id))?.transcript_json ?? "[]").length, { timeout: 40_000 }).toBeGreaterThan(0);
  const pushed = (await latest(device.id))!;
  expect(pushed).toMatchObject({ title: "Q4 pricing sync", ended_at: null, stt_engine: "cloud · nova-3 · phone" });

  // A killed tab: the library offers the unfinished meeting, which already lists under the phone.
  const id = pushed.id;
  const kept = JSON.parse((await latest(device.id))!.transcript_json).length;
  const keptNewest = await newest(page);
  await page.reload();
  // The tab comes back on the record screen, which offers the meeting too. Reopening Yogurt itself lands on the library.
  await expect(page.getByText("Q4 pricing sync stopped recording")).toBeVisible();
  await expect(page.getByRole("button", { name: "Start recording" })).toBeVisible();
  await page.goto("/");
  await expect(page.getByText("Q4 pricing sync stopped recording")).toBeVisible();
  await expect(page.getByRole("button", { name: /Q4 pricing sync/ })).toBeVisible();
  await shot(page, info, "r6-resume", false);

  await page.getByRole("button", { name: "Resume" }).click();
  await expect(page.getByRole("button", { name: "End meeting" })).toBeVisible();
  await expect.poll(() => newest(page), { timeout: 15_000 }).toBeGreaterThan(keptNewest);
  // The lines from before the reload are still there, above the new ones.
  await sheet(page).getByRole("button", { name: "Raise transcript" }).click();
  expect(await finals(page).count()).toBeGreaterThan(kept);
  await sheet(page).getByRole("button", { name: "Lower transcript" }).click();
  if (!phone) await shot(page, info, "r7-resumed", false);

  await page.getByRole("button", { name: "End meeting" }).click();
  await expect(page.getByRole("heading", { name: "Q4 pricing sync" })).toBeVisible();
  const ended = (await latest(device.id))!;
  expect(ended.id).toBe(id);
  expect(ended.ended_at).toBeGreaterThan(0);
  expect(ended.notes_md).toContain("ask finance re annual cap");
  const lines: { channel: string; text: string; ts_ms: number }[] = JSON.parse(ended.transcript_json);
  expect(lines.length).toBeGreaterThan(kept);
  expect(lines.every((l) => l.channel === "them" && /^Heard line \d+ in full\.$/.test(l.text))).toBe(true);
  expect(lines.map((l) => l.ts_ms)).toEqual([...lines.map((l) => l.ts_ms)].sort((a, b) => a - b));
  expect(Object.keys(lines[0])).toEqual(["channel", "text", "ts_ms"]);

  // The meeting view shows the phone's lines with no speaker, and the notes as typed.
  await page.getByRole("tab", { name: "Transcript" }).click();
  await expect(page.getByRole("tabpanel").getByText(lines[0].text)).toBeVisible();
  await expect(page.getByRole("tabpanel").getByText("Them")).toHaveCount(0);
  await shot(page, info, "r8-ended-transcript");
  await page.getByRole("tab", { name: "Notes" }).click();
  await expect(page.getByRole("tabpanel").getByText("ask finance re annual cap")).toBeVisible();

  // Back in the library, the meeting lists under the phone's own device pill, and nothing is left to resume.
  if (phone) await page.getByRole("button", { name: /^← / }).click();
  await page.getByRole("button", { name: name, exact: true }).click();
  await expect(page.getByTestId("list-row")).toHaveCount(1);
  await expect(page.getByTestId("list-row")).toContainText("Q4 pricing sync");
  await expect(page.getByText(/stopped recording/)).toHaveCount(0);
  await shot(page, info, "r9-library-phone", false);
});
