import { expect, test, type Page } from "@playwright/test";
import { PIN, isPhone, onboard, query, reset, shot, signIn, typePin } from "./helpers";

// The second test signs in as the owner the first one creates.
test.describe.configure({ mode: "serial" });
test.beforeAll(reset);

/**
 * The Android app's bridge as the Transcription page and the recorder use it: models download in a moment, a test answers with a `model-test` event,
 * and a start is kept in `window.starts` so the spec can play the recorder's events.
 */
function fakeApp() {
  type M = { name: string; size_mb: number; downloaded: boolean; downloading: boolean; bytes: number; total: number; waiting: string | null; error: string | null };
  const sizes: [string, number][] = [["tiny.en", 104], ["base.en", 161], ["small.en", 376], ["parakeet-v3", 671]];
  const models: M[] = sizes.map(([name, mb]) => ({ name, size_mb: mb, downloaded: false, downloading: false, bytes: 0, total: mb * 1e6, waiting: null, error: null }));
  const emit = (detail: object) => window.dispatchEvent(new CustomEvent("yogurt-native", { detail }));
  const ok = '{"ok":true}';
  const w = window as unknown as { starts: unknown[]; emit: typeof emit; YogurtAndroid: object };
  w.starts = [];
  w.emit = emit;
  w.YogurtAndroid = {
    start: (json: string) => (w.starts.push(JSON.parse(json)), setTimeout(() => emit({ type: "state", state: "live" }), 50), ok),
    setAuth: () => undefined,
    stop: () => setTimeout(() => emit({ type: "state", state: "stopped" }), 50),
    status: () => JSON.stringify({ recording: false, meetingId: null }),
    lines: () => "[]",
    appVersion: () => "0.1.0",
    models: () => JSON.stringify(models),
    downloadModel: (name: string) => {
      const m = models.find((x) => x.name === name)!;
      Object.assign(m, { downloading: true, bytes: m.total * 0.4 });
      setTimeout(() => Object.assign(m, { downloading: false, downloaded: true, bytes: m.total }), 1500);
      return ok;
    },
    cancelDownload: () => undefined,
    deleteModel: (name: string) => (Object.assign(models.find((x) => x.name === name)!, { downloaded: false, bytes: 0 }), ok),
    testModel: (name: string) => setTimeout(() => emit({ type: "model-test", name, ok: true, model: `${name} · loaded and ran in 812ms` }), 50),
  };
}

const card = (page: Page, name: "cloud" | "local") => page.getByTestId(`${name}-stt-card`);

test("in the Android app, Settings opens Transcription, a model downloads, and the next recording runs on the phone", async ({ page }, info) => {
  await page.addInitScript(fakeApp);
  await onboard(page);
  await page.goto("/#/settings");
  await expect(page.getByText("Cloud · Deepgram")).toBeVisible();
  await shot(page, info, "settings-transcription-row", false);
  await (isPhone(page) ? page.getByRole("button", { name: /^Transcription/ }) : page.getByRole("button", { name: "Change", exact: true })).click();
  await expect(page).toHaveURL(/#\/settings\/transcription$/);
  await expect(card(page, "cloud").getByText("In use")).toBeVisible();
  await expect(card(page, "local").getByRole("button", { name: "Use Local" })).toBeDisabled();
  await expect(card(page, "local").getByText("Download a model first")).toBeVisible();

  await page.getByRole("button", { name: "Download parakeet-v3 (671 MB)" }).click();
  await expect(page.getByText(/Downloading parakeet-v3 · 268 MB of 671 MB/)).toBeVisible();
  await shot(page, info, "transcription-downloading", false);
  await expect(page.getByRole("button", { name: "Select parakeet-v3" })).toBeVisible();

  await card(page, "local").getByRole("button", { name: "Use Local" }).click();
  await expect(card(page, "local").getByText("In use")).toBeVisible();
  await expect(card(page, "cloud").getByRole("button", { name: "Use Cloud" })).toBeVisible();
  await page.getByRole("button", { name: "Test connection for parakeet-v3" }).click();
  await expect(page.getByText("✓ Connection works · answered as parakeet-v3 · loaded and ran in 812ms")).toBeVisible();
  await shot(page, info, "transcription-local", false);

  await page.getByRole("button", { name: "Settings" }).click();
  await expect(page.getByText("Local · parakeet-v3 · on this phone")).toBeVisible();

  await page.goto("/#/record");
  await expect(page.getByText("This phone transcribes it, so the audio never leaves it")).toBeVisible();
  await shot(page, info, "ready-local", false);
  await page.getByRole("button", { name: "Start recording" }).click();
  await expect(page.getByRole("button", { name: "End meeting" })).toBeVisible();
  expect(await page.evaluate(() => (window as unknown as { starts: { model: string }[] }).starts.map((s) => s.model))).toEqual(["parakeet-v3"]);
  await page.evaluate(() => {
    const w = window as unknown as { emit: (d: object) => void; starts: { meetingId: string }[] };
    w.emit({ type: "line", meetingId: w.starts[0].meetingId, index: 0, line: { channel: "them", text: "Dana's proposal is $18 a month.", ts_ms: 4_000 } });
    w.emit({ type: "speaking", speaking: true });
  });
  const sheet = page.getByRole("region", { name: "Live transcript" });
  await expect(sheet.getByText("local")).toBeVisible();
  await expect(sheet.getByLabel("Someone is speaking")).toBeVisible();
  await page.evaluate(() => (window as unknown as { emit: (d: object) => void }).emit({ type: "backlog", ms: 48_000 }));
  await expect(page.getByText("Transcription is 48 seconds behind.")).toBeVisible();
  await expect(sheet.getByText("48s behind")).toBeVisible();
  await shot(page, info, "recording-local", false);
  await page.evaluate(() => (window as unknown as { emit: (d: object) => void }).emit({ type: "backlog", ms: 0 }));
  await expect(page.getByText("Transcription is 48 seconds behind.")).toHaveCount(0);

  await expect
    .poll(async () => (await query<{ e: string }>("select v.body->'meeting'->>'stt_engine' as e from yogurt_versions v join yogurt_devices d on d.id = v.device_id where d.kind = 'phone' order by v.push_id desc limit 1"))[0]?.e)
    .toBe("local · parakeet-v3 · phone");
});

test("in a browser, Local is there but needs the Android app", async ({ page }, info) => {
  await signIn(page);
  await typePin(page, PIN);
  await page.getByRole("button", { name: "Unlock" }).click();
  await expect(page.getByRole("button", { name: "Unlock" })).toHaveCount(0);
  await page.goto("/#/settings/transcription");
  await expect(card(page, "local").getByText(/Needs the Yogurt app for Android/)).toBeVisible();
  await expect(card(page, "local").getByRole("button", { name: "Use Local" })).toBeDisabled();
  await expect(card(page, "cloud").getByText("In use")).toBeVisible();
  await shot(page, info, "transcription-browser", false);
});
