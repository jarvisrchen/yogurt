import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync } from "node:fs";
import { createServer, type Server } from "node:http";
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

// A stand-in for the LLM provider at the address playwright.config.ts gives the enhance and chat functions. It holds the second half of an enhance until the spec lets it go.
// A chat question gets the last transcript line it was sent, quoted after a first piece, and `chatHeld` holds that quote.
const LLM_PORT = Number(process.env.YOGURT_CLOUD_E2E_LLM_PORT ?? 5192);
const REPLY = "template: general\n\n## Pricing\n- pro tier at $18?\n- ask finance re annual cap\n\n## Action items\n- Dana sends the annual price to finance\n";
let release: () => void = () => undefined;
let held = new Promise<void>((r) => (release = r));
let chatHeld = Promise.resolve();
let chats = 0;
let enhances = 0;
let llm: Server;

test.describe.configure({ mode: "serial" });
test.beforeAll(async () => {
  await reset();
  await dg.start();
  llm = createServer(async (req, res) => {
    let body = "";
    for await (const c of req) body += c;
    res.on("error", () => undefined);
    res.writeHead(200, { "content-type": "text/event-stream" });
    const sent = (content: string) => res.write(`data: ${JSON.stringify({ choices: [{ delta: { content }, finish_reason: null }] })}\n\n`);
    const transcript = (JSON.parse(body).messages as { content: string }[]).find((m) => m.content.startsWith("TRANSCRIPT SO FAR"));
    if (transcript) {
      chats++;
      sent("The last line was: ");
      await chatHeld;
      sent(transcript.content.trim().split("\n").at(-1)!.replace(/^\[\d\d:\d\d\] them: /, ""));
      return res.end("data: [DONE]\n\n");
    }
    enhances++;
    const pieces = REPLY.match(/[\s\S]{1,24}/g)!;
    for (const [i, content] of pieces.entries()) {
      if (i === Math.floor(pieces.length / 2)) await held;
      sent(content);
    }
    res.end(`data: ${JSON.stringify({ choices: [{ delta: {}, finish_reason: "stop" }] })}\n\ndata: [DONE]\n\n`);
  });
  await new Promise<void>((r) => llm.listen(LLM_PORT, "127.0.0.1", r));
});
test.afterAll(async () => {
  await dg.stop();
  await new Promise<void>((r) => llm.close(() => r()));
});

const sheet = (page: Page) => page.getByRole("region", { name: "Live transcript" });
const finals = (page: Page) => sheet(page).locator('[data-final="true"]');
const ticker = (page: Page) => sheet(page).getByRole("button", { name: "Show transcript" });
/** The number of the newest final line the fake sent that the sheet shows. The peek shows only two rows, and the ticker one line, which may be the next line's interim words. */
const newest = async (page: Page) => {
  if (await ticker(page).isVisible()) {
    const [, n, full] = /Heard line (\d+)( in full)?/.exec((await ticker(page).textContent()) ?? "") ?? [];
    return n ? Number(n) - (full ? 0 : 1) : 0;
  }
  return Number(/Heard line (\d+)/.exec((await finals(page).last().textContent()) ?? "")?.[1] ?? 0);
};

type Meeting = { id: string; title: string; ended_at: number | null; notes_md: string; transcript_json: string; stt_engine: string; enriched_md: string | null; llm_model: string | null; template: string | null };
type Version = { body: { meeting: Meeting } };
const latest = async (deviceId: string) =>
  (await query<Version>("select body from yogurt_versions where device_id = $1 order by push_id desc limit 1", [deviceId]))[0]?.body.meeting;
const all = async (deviceId: string) => (await query<Version>("select body from yogurt_versions where device_id = $1 order by push_id", [deviceId])).map((v) => v.body.meeting);

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
  // A phone's keyboard shrinks the page, and the sheet keeps just the newest line until it goes down.
  const screen = page.viewportSize()!;
  const keyboard = (up: boolean) => page.setViewportSize({ width: screen.width, height: up ? 500 : screen.height });
  if (phone) {
    await keyboard(true);
    await expect(ticker(page)).toBeVisible();
    await keyboard(false);
    await expect(ticker(page)).toBeHidden();
  }
  await expect.poll(() => newest(page), { timeout: 15_000 }).toBeGreaterThanOrEqual(2);
  await shot(page, info, "r3-recording", false);

  // A question mid-recording pushes the meeting first, still open, so the answer quotes the line just heard. The transcript keeps arriving under the chat.
  const ask = page.getByRole("button", { name: "Ask this meeting" });
  const chat = page.getByRole("region", { name: "Ask the meeting chat" });
  const input = chat.getByPlaceholder("Ask this meeting…");
  await ask.click();
  await expect(ticker(page)).toBeVisible();
  const heard = await newest(page);
  await input.fill("What was just said?");
  await input.press("Enter");
  const answer = chat.getByText(/^The last line was: Heard line \d+ in full\.$/);
  await expect(answer).toBeVisible();
  expect(Number(/Heard line (\d+)/.exec((await answer.textContent())!)![1])).toBeGreaterThanOrEqual(heard);
  const atAsk = (await latest(device.id))!;
  expect(atAsk.ended_at).toBeNull();
  await expect.poll(() => newest(page), { timeout: 15_000 }).toBeGreaterThan(heard);
  await shot(page, info, "r3b-chat", false);

  // A push that fails shows its error in the chat, and the question never reaches the model.
  const chatsBefore = chats;
  const refuse = "**/rest/v1/rpc/yogurt_start_push";
  await page.route(refuse, (r) => r.fulfill({ status: 500, contentType: "application/json", body: JSON.stringify({ message: "Push refused." }) }));
  await input.fill("And now?");
  await input.press("Enter");
  await expect(chat.getByText("[Could not save the meeting before asking. Push refused.]")).toBeVisible();
  expect(chats).toBe(chatsBefore);
  await page.unroute(refuse);
  await chat.getByRole("button", { name: "Collapse chat" }).click();
  await expect(ask).toBeVisible();

  await sheet(page).getByRole("button", { name: "Raise transcript" }).click();
  await expect(sheet(page).getByRole("button", { name: "Lower transcript" })).toBeVisible();
  await expect(finals(page).first()).toBeInViewport();
  await shot(page, info, "r4-sheet-up", false);
  await sheet(page).getByRole("button", { name: "Lower transcript" }).click();
  await expect(sheet(page).getByRole("button", { name: "Raise transcript" })).toBeVisible();

  // A slow drag follows the finger and leaves the sheet where it was let go, and a flick down sends it back to the peek.
  const height = async () => (await sheet(page).boundingBox())!.height;
  const peek = 128;
  await expect.poll(height).toBe(peek);
  const grip = (await sheet(page).getByRole("button", { name: "Raise transcript" }).boundingBox())!;
  const [x, y] = [grip.x + grip.width / 2, grip.y + grip.height / 2];
  // A finger that wobbles a few pixels still taps.
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x, y - 3);
  await page.mouse.up();
  await expect(sheet(page).getByRole("button", { name: "Lower transcript" })).toBeVisible();
  await sheet(page).getByRole("button", { name: "Lower transcript" }).click();
  await expect.poll(height).toBe(peek);
  await page.mouse.move(x, y);
  await page.mouse.down();
  for (let i = 1; i <= 12; i++) {
    await page.mouse.move(x, y - i * 20);
    await page.waitForTimeout(16);
  }
  expect(await height()).toBeCloseTo(peek + 240, -1);
  await page.waitForTimeout(150);
  await page.mouse.up();
  await page.waitForTimeout(400);
  expect(await height()).toBeCloseTo(peek + 240, -1);
  await expect(sheet(page).getByRole("button", { name: "Lower transcript" })).toBeVisible();
  await expect(ask).toBeVisible();
  await shot(page, info, "r4b-sheet-mid", false);
  // The chat shrinks the sheet to its ticker, and a tap on the ticker closes the chat and puts the sheet back where it was.
  await ask.click();
  await expect.poll(height).toBe(48);
  await shot(page, info, "r4c-ticker", false);
  // About what the page keeps above an open keyboard, which resizes it.
  if (phone) {
    await keyboard(true);
    await shot(page, info, "r4d-keyboard", false);
    await keyboard(false);
  }
  await ticker(page).click();
  await expect(chat).toBeHidden();
  await expect.poll(height).toBeCloseTo(peek + 240, -1);
  const mid = (await sheet(page).getByRole("button", { name: "Lower transcript" }).boundingBox())!;
  await page.mouse.move(x, mid.y + 10);
  await page.mouse.down();
  await page.mouse.move(x, mid.y + 200, { steps: 4 });
  await page.mouse.up();
  await expect.poll(height).toBeCloseTo(peek, 0);
  await expect(ask).toBeVisible();

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

  // The 30 second push carries the transcript so far, past what the question's push had, with ended_at still null.
  await expect.poll(async () => JSON.parse((await latest(device.id))?.transcript_json ?? "[]").length, { timeout: 40_000 }).toBeGreaterThan(JSON.parse(atAsk.transcript_json).length);
  const pushed = (await latest(device.id))!;
  expect(pushed).toMatchObject({ title: "Q4 pricing sync", ended_at: null, stt_engine: "cloud · nova-3 · phone" });

  // A killed tab: the library offers the unfinished meeting, which already lists under the phone.
  const id = pushed.id;
  const kept = JSON.parse((await latest(device.id))!.transcript_json).length;
  const keptNewest = await newest(page);
  await page.reload();
  // The tab comes back on the record screen, which offers the meeting too. Reopening Yogurt itself lands on the library.
  await expect(page.getByText("Q4 pricing sync stopped recording")).toBeVisible();
  // A new recording would replace the unfinished one, so it waits until that one is resumed or ended.
  await expect(page.getByRole("button", { name: "Start recording" })).toBeDisabled();
  await expect(page.getByText("Resume or end Q4 pricing sync first.")).toBeVisible();
  await shot(page, info, "r6a-ready-unfinished", false);
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

  // End works while a chat answer is still streaming, which the closing record screen drops.
  // The question's push counts with End's, since End skips a push when no line came in after it.
  const versionsBefore = (await all(device.id)).length;
  let releaseChat: () => void = () => undefined;
  chatHeld = new Promise<void>((r) => (releaseChat = r));
  await ask.click();
  await input.fill("What was just said?");
  await input.press("Enter");
  await expect(chat.getByText("The last line was:")).toBeVisible();

  // End saves the last of the transcript still open, then opens the meeting under the enhance banner while the model streams.
  held = new Promise<void>((r) => (release = r));
  await page.getByRole("button", { name: "End meeting" }).click();
  releaseChat();
  await expect(page.getByRole("heading", { name: "Q4 pricing sync" })).toBeVisible();
  await expect(page.getByText("Weaving your notes into the transcript…")).toBeVisible();
  await expect(page.getByRole("tab", { name: "Enhanced", selected: true })).toBeVisible();
  await expect(page.getByRole("tabpanel").getByText("Pricing", { exact: true })).toBeVisible();
  await shot(page, info, "r8-enhancing");
  const whileEnhancing = (await all(device.id)).slice(versionsBefore);
  expect(whileEnhancing.length).toBeGreaterThan(0);
  expect(whileEnhancing.every((m) => m.ended_at === null && m.enriched_md === null)).toBe(true);
  expect(JSON.parse(whileEnhancing.at(-1)!.transcript_json).length).toBeGreaterThan(kept);
  // The ended push fails once. Retry pushes the enhance already done, without asking the model again.
  const enhancesBefore = enhances;
  await page.route(refuse, (r) => r.fulfill({ status: 500, contentType: "application/json", body: JSON.stringify({ message: "Push refused." }) }));
  release();
  await expect(page.getByText("Not saved to your Cloud yet. Push refused.")).toBeVisible();
  await page.unroute(refuse);
  await page.getByTestId("enhancing-banner-retry").click();
  await expect(page.getByText("Weaving your notes into the transcript…")).toHaveCount(0);
  expect(enhances).toBe(enhancesBefore);
  await expect(page.getByRole("tabpanel").getByText("Dana sends the annual price to finance")).toBeVisible();
  await expect(page.getByRole("button", { name: "Re-enhance" })).toBeVisible();
  await shot(page, info, "r9-enhanced");
  const ended = (await latest(device.id))!;
  expect(ended.id).toBe(id);
  expect(ended.ended_at).toBeGreaterThan(0);
  expect(ended).toMatchObject({ llm_model: "fake-model", template: "general" });
  expect(ended.enriched_md).toContain("Dana sends the annual price to finance");
  expect((await all(device.id)).filter((m) => m.ended_at !== null)).toHaveLength(1);
  expect(ended.notes_md).toContain("ask finance re annual cap");
  const lines: { channel: string; text: string; ts_ms: number }[] = JSON.parse(ended.transcript_json);
  expect(lines.length).toBeGreaterThan(kept);
  expect(lines.every((l) => l.channel === "them" && /^Heard line \d+ in full\.$/.test(l.text))).toBe(true);
  // The fake's lines each cover 2 seconds, so a step of any other size lost or repeated audio. Only the killed tab may lose some: the dropped socket loses and repeats none.
  const steps = lines.slice(1).map((l, i) => l.ts_ms - lines[i].ts_ms);
  expect(steps.filter((s) => s !== 2_000)).toEqual([expect.any(Number)]);
  expect(Math.min(...steps)).toBe(2_000);
  expect(Object.keys(lines[0])).toEqual(["channel", "text", "ts_ms"]);

  // The meeting view shows the phone's lines with no speaker, and the notes as typed.
  await page.getByRole("tab", { name: "Transcript" }).click();
  await expect(page.getByRole("tabpanel").getByText(lines[0].text)).toBeVisible();
  await expect(page.getByRole("tabpanel").getByText("Them")).toHaveCount(0);
  await shot(page, info, "r10-ended-transcript");
  await page.getByRole("tab", { name: "Notes" }).click();
  await expect(page.getByRole("tabpanel").getByText("ask finance re annual cap")).toBeVisible();

  // Back in the library, the meeting lists under the phone's own device pill, and nothing is left to resume.
  if (phone) await page.getByRole("button", { name: "All devices" }).click();
  await page.getByRole("button", { name: name, exact: true }).click();
  await expect(page.getByTestId("list-row")).toHaveCount(1);
  await expect(page.getByTestId("list-row")).toContainText("Q4 pricing sync");
  await expect(page.getByText(/stopped recording/)).toHaveCount(0);
  await shot(page, info, "r11-library-phone", false);
});
