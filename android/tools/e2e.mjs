// End-to-end test of the debug app on a booted emulator, against fake-cloud.mjs reached through `adb reverse`.
// Run from the repo root after `just android`, with one emulator booted: node android/tools/e2e.mjs
// It wipes the app's data, so it never runs against a phone with a real Cloud set up.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fakeCloud } from "./fake-cloud.mjs";

const SDK = process.env.ANDROID_HOME ?? path.join(os.homedir(), "Library/Android/sdk");
const ADB = path.join(SDK, "platform-tools/adb");
const PKG = "com.jarvisrchen.yogurt";
const APK = new URL("../app/build/outputs/apk/debug/app-debug.apk", import.meta.url).pathname;
const PORT = 8787;
// The debug build's versionName, which Gradle takes from the workspace.
const VERSION = /\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m.exec(fs.readFileSync(new URL("../../Cargo.toml", import.meta.url), "utf8"))[1];

const adb = (...a) => execFileSync(ADB, a, { encoding: "utf8", maxBuffer: 64 << 20 });
const sh = (cmd) => adb("shell", cmd);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (m) => console.log(`${new Date().toISOString().slice(11, 19)} ${m}`);

let failures = 0;
function check(name, ok, detail = "") {
  console.log(`${ok ? "PASS" : "FAIL"} ${name}${detail ? ` (${detail})` : ""}`);
  if (!ok) failures++;
}
async function until(what, fn, ms = 30_000) {
  const end = Date.now() + ms;
  for (;;) {
    const v = await fn();
    if (v) return v;
    if (Date.now() > end) throw new Error(`timed out waiting for ${what}`);
    await sleep(500);
  }
}

/** A 16 kHz mono wav whose samples hold their own second, so the fake can say which audio a line came from. */
function writeWav(file, seconds) {
  const n = seconds * 16_000;
  const b = Buffer.alloc(44 + n * 2);
  b.write("RIFF", 0);
  b.writeUInt32LE(36 + n * 2, 4);
  b.write("WAVEfmt ", 8);
  b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20);
  b.writeUInt16LE(1, 22);
  b.writeUInt32LE(16_000, 24);
  b.writeUInt32LE(32_000, 28);
  b.writeUInt16LE(2, 32);
  b.writeUInt16LE(16, 34);
  b.write("data", 36);
  b.writeUInt32LE(n * 2, 40);
  for (let i = 0; i < n; i++) b.writeInt16LE(Math.floor(i / 16_000), 44 + i * 2);
  fs.writeFileSync(file, b);
}

function screen() {
  sh("uiautomator dump /sdcard/yogurt-ui.xml >/dev/null");
  return adb("exec-out", "cat", "/sdcard/yogurt-ui.xml");
}
/** Taps the first view whose text or resource id matches. */
function tap(match) {
  const xml = screen();
  const node = xml.split("<node ").find((n) => n.includes(`text="${match}"`) || n.includes(`resource-id="${PKG}:id/${match}"`));
  if (!node) throw new Error(`nothing on screen matches ${match}`);
  const [, x1, y1, x2, y2] = node.match(/bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/).map(Number);
  sh(`input tap ${(x1 + x2) >> 1} ${(y1 + y2) >> 1}`);
}

const store = () => {
  const out = adb("exec-out", "run-as", PKG, "cat", "files/recording/lines.jsonl").trim();
  return out ? out.split("\n").map((l) => JSON.parse(l)) : [];
};
const serviceUp = () => sh(`dumpsys activity services ${PKG}`).includes("RecorderService");
/** Lines must read second 0, 2, 4, ...: a dropped socket that lost or repeated audio breaks the run. The last may be a partial block. */
function contiguous(lines) {
  const bad = lines.findIndex((l, i) => l.text !== `second ${2 * i}` || l.channel !== "them");
  return bad === -1 ? "" : `line ${bad} is "${lines[bad].text}"`;
}
const sameLines = (a, b) => JSON.stringify(a) === JSON.stringify(b);

const cloud = fakeCloud({ port: PORT, apk: APK, log: (m) => log(`fake: ${m}`) });
const page = () => cloud.state.page;

async function main() {
  log("installing");
  adb("install", "-r", APK);
  sh(`pm clear ${PKG}`);
  const wav = path.join(os.tmpdir(), "yogurt-e2e.wav");
  writeWav(wav, 600);
  adb("push", wav, "/data/local/tmp/yogurt-e2e.wav");
  sh(`run-as ${PKG} sh -c 'mkdir -p files && cp /data/local/tmp/yogurt-e2e.wav files/test.wav'`);
  sh(`pm grant ${PKG} android.permission.RECORD_AUDIO`);
  sh(`pm grant ${PKG} android.permission.POST_NOTIFICATIONS`);
  adb("reverse", `tcp:${PORT}`, `tcp:${PORT}`);
  await cloud.listen();
  sh("input keyevent KEYCODE_WAKEUP");
  sh("wm dismiss-keyguard");
  sh("cmd statusbar collapse");

  log("first launch");
  sh(`am start -W -n ${PKG}/.MainActivity --es wav test.wav --es releasesUrl ${cloud.url}/releases/latest`);
  await until("the Cloud address screen", () => screen().includes("Connect your Cloud"));
  tap("address");
  sh("input text http://example.com");
  tap("Continue");
  await until("the http refusal", () => screen().includes("Type an https address"));
  check("an http Cloud other than the Mac is refused", true);
  tap("address");
  sh("input keyevent KEYCODE_MOVE_END");
  for (let i = 0; i < 20; i++) sh("input keyevent KEYCODE_DEL");
  sh(`input text ${cloud.url}`);
  tap("Continue");
  await until("the test page", () => page());
  check("the bridge reports the app version", page().version === VERSION, page().version);
  await until("the update banner", () => screen().includes("Yogurt for Android 99.0.0 is ready."));
  check("the update banner shows against a newer fake release", true);

  log("bridge guards");
  cloud.command({ cmd: "start-raw", json: JSON.stringify({ meetingId: "m-0", startedAtMs: Date.now(), tokenUrl: "https://evil.test/token", auth: { jwt: "j", browserToken: "b" } }) });
  const guard = await until("the refused start", () => page().events.find((e) => e.type === "start"));
  check("start refuses a tokenUrl off the Cloud", guard.result.ok === false, guard.result.error);

  log("recording");
  cloud.command({ cmd: "start", meetingId: "m-1" });
  await until("three lines", () => page().status?.recording && page().lines.length >= 3);
  check("start answers ok", page().events.find((e) => e.type === "start" && e.result.ok));
  check("lines arrive live", page().lines.length >= 3, `${page().lines.length} lines`);
  const shade = sh("dumpsys notification --noredact");
  const ours = shade.split("NotificationRecord").find((r) => r.includes(`pkg=${PKG}`)) ?? "";
  check("the Recording notification is posted", /android\.title=String \(Recording\)/.test(ours));
  check("the notification has a Stop action", /actions=\{[\s\S]*"Stop"/.test(ours) || /\[0\] "Stop"/.test(ours));
  check("the notification shows a chronometer", /android\.showChronometer=Boolean \(true\)/.test(ours));
  const services = sh(`dumpsys activity services ${PKG}`);
  check("the service runs in the foreground as microphone", services.includes("isForeground=true") && /foregroundServiceType=0x00000080|types=0x00000080/.test(services));

  log("home and screen off for 75 s");
  sh("input keyevent KEYCODE_HOME");
  await sleep(1_000);
  sh("input keyevent KEYCODE_POWER");
  await sleep(2_000);
  const asleep = /mWakefulness=(Asleep|Dozing)/.test(sh("dumpsys power"));
  const before = store().length;
  await sleep(75_000);
  const after = store().length;
  check("the screen was off", asleep);
  check("lines keep arriving while backgrounded and locked", after - before >= 30, `${before} -> ${after} in 75 s`);
  check("the service survives home and lock", serviceUp());

  log("back to the app");
  sh("input keyevent KEYCODE_WAKEUP");
  sh("wm dismiss-keyguard");
  sh(`am start -n ${PKG}/.MainActivity`);
  await until("the page to catch up", () => page().lines.length >= after && sameLines(page().lines, store().slice(0, page().lines.length)));
  check("the page holds every line the service kept, in order", true, `${page().lines.length} lines`);

  log("killing the fake server for 8 s");
  const sockets = cloud.state.sockets;
  await cloud.kill();
  await sleep(8_000);
  await cloud.listen();
  await until("a new socket", () => cloud.state.sockets > sockets, 60_000);
  const resumed = store().length;
  await until("lines after the reconnect", () => store().length >= resumed + 8, 60_000);
  const events = page().events;
  check("the page saw reconnecting then live", events.some((e) => e.state === "reconnecting") && events.findLastIndex((e) => e.state === "live") > events.findIndex((e) => e.state === "reconnecting"));
  check("the reconnect lost and repeated no line", !contiguous(store()), contiguous(store()) || `${store().length} lines`);
  check("the reconnect minted a fresh token", cloud.state.tokens >= 2, `${cloud.state.tokens} tokens`);

  // The app's session expires with the screen locked, where the page no longer renews it on its own, and then the socket drops.
  log("a session that expires while locked");
  cloud.state.jwt = "jwt-2";
  cloud.command({ cmd: "rotate", jwt: "jwt-2" });
  await until("the page to hold the new session", () => page().events.some((e) => e.type === "rotated"));
  sh("input keyevent KEYCODE_POWER");
  await sleep(2_000);
  const lockedSockets = cloud.state.sockets;
  const lockedLines = store().length;
  await cloud.kill();
  await sleep(3_000);
  await cloud.listen();
  await until("a socket on the renewed session", () => cloud.state.sockets > lockedSockets, 60_000);
  await until("lines after it", () => store().length >= lockedLines + 5, 60_000);
  check("a refused session asks the locked page for a fresh one", cloud.state.refused >= 1 && page().events.some((e) => e.type === "auth"), `${cloud.state.refused} refused`);
  check("the renewed session reconnects with no lost or repeated line", !contiguous(store()), contiguous(store()) || `${store().length} lines`);
  sh("input keyevent KEYCODE_WAKEUP");
  sh("wm dismiss-keyguard");
  sh(`am start -n ${PKG}/.MainActivity`);

  log("reloading the page with 5 lines missing");
  const keep = page().lines.length - 5;
  cloud.command({ cmd: "keep", n: keep });
  await until("the trim", () => page().lines.length === keep);
  const oldPage = page().pageId;
  cloud.command({ cmd: "reload" });
  await until("the reloaded page", () => page().pageId !== oldPage);
  const load = page().events.find((e) => e.why === "load");
  check("lines(meetingId, n) after a reload starts at the count the page had", load.from === keep, `from ${load.from}`);
  check("lines(meetingId, n) returns exactly the missing lines", sameLines(page().lines, store().slice(0, page().lines.length)) && load.got >= 5, `got ${load.got}`);

  // uiautomator cannot dump the shade while the notification's chronometer ticks, since the screen is never idle.
  // So this sends the intent the Stop button's PendingIntent carries, as the app's own uid, which is what a tap does.
  log("Stop in the notification");
  check("the Stop action starts the service", /"Stop" -> PendingIntent\{[^}]*com\.jarvisrchen\.yogurt startService/.test(sh("dumpsys notification --noredact")));
  sh(`run-as ${PKG} am startservice --user 0 -n ${PKG}/.RecorderService -a ${PKG}.STOP`);
  await until("the stop", () => !page().status?.recording && page().events.some((e) => e.state === "stopped"), 15_000);
  await until("the service to go", () => !serviceUp(), 10_000);
  check("Stop in the notification stops the recorder and tells the page", true);
  check("the notification is gone", !sh("dumpsys notification --noredact").includes(`pkg=${PKG}`));
  check("after Stop the page holds every line", sameLines(page().lines, store()) && !contiguous(store()), `${store().length} lines`);

  log("resuming the stopped meeting");
  const held = page().lines.length;
  cloud.command({ cmd: "start", meetingId: "m-1" });
  await until("lines after the resume", () => page().status?.recording && page().lines.length >= held + 2);
  const resume = page().events.findIndex((e) => e.type === "start" && e.meetingId === "m-1");
  const resumedAt = page().events.slice(resume).find((e) => e.type === "line");
  check("a resume asks to carry on from the lines the page holds", page().events[resume]?.have === held, `have ${page().events[resume]?.have}`);
  check("a resumed meeting's lines carry on its numbering", resumedAt?.index === held, `first index ${resumedAt?.index}, page held ${held}`);
  const beforeReload = page().pageId;
  cloud.command({ cmd: "reload" });
  await until("the reloaded page", () => page().pageId !== beforeReload);
  await until("lines after the reload", () => page().lines.length >= store().length - 1 && page().lines.length > held + 2);
  check("after a resume and a reload the page holds every line, in order", sameLines(page().lines, store().slice(0, page().lines.length)), `${page().lines.length} of ${store().length} lines`);
  cloud.command({ cmd: "stop" });
  await until("the resumed meeting to stop", () => page().events.some((e) => e.type === "stopped-call"), 15_000);
  const resumedPage = page().pageId;
  cloud.command({ cmd: "reload" });
  await until("a fresh page", () => page().pageId !== resumedPage);

  log("stop() from the page");
  cloud.command({ cmd: "start", meetingId: "m-2" });
  await until("two lines", () => page().status?.recording && page().lines.length >= 2);
  cloud.command({ cmd: "stop" });
  const returned = await until("stop() to return", () => page().events.find((e) => e.type === "stop-returned"), 15_000);
  check("stop() returns at once", returned.ms < 200, `${returned.ms} ms`);
  const stopped = await until("the stopped event", () => page().events.find((e) => e.type === "stopped-call"), 15_000);
  check("the stopped event comes once the recording has ended", stopped.status.recording === false);
  check("the lines read right after the stopped event are all of them", sameLines(page().lines, store()) && page().lines.length === stopped.status.lines, `${page().lines.length} lines`);
  check("a new start numbers from 0 again", page().lines[0]?.text === "second 0" && !contiguous(store()));

  // Revoking the mic kills the app, and the store still holds m-2 when m-3 starts, since the permission dialog comes first.
  log("a start that asks for the mic");
  const killed = page().pageId;
  sh(`pm revoke ${PKG} android.permission.RECORD_AUDIO`);
  sh(`am start -n ${PKG}/.MainActivity`);
  await until("the page after the restart", () => page().pageId !== killed && page().meetingId === "m-2" && !page().status?.recording, 30_000);
  cloud.command({ cmd: "start", meetingId: "m-3" });
  await until("the permission dialog", () => /While using the app/.test(screen()), 15_000);
  const asked = page().events.findLast((e) => e.type === "catchup" && e.why === "start");
  check("while the dialog is up the page reads none of the previous meeting's lines", asked?.got === 0 && page().lines.length === 0, `got ${asked?.got}`);
  tap("While using the app");
  await until("m-3's lines", () => page().status?.recording && page().status.meetingId === "m-3" && page().lines.length >= 2);
  check("the new meeting's lines start at its first", page().lines[0]?.text === "second 0" && sameLines(page().lines, store().slice(0, page().lines.length)), `${page().lines.length} lines`);
  cloud.command({ cmd: "stop" });
  await until("m-3 to stop", () => page().events.some((e) => e.type === "stopped-call"), 15_000);

  log("update");
  sh(`am start -n ${PKG}/.MainActivity`);
  await until("the update banner", () => screen().includes("Yogurt for Android 99.0.0 is ready."));
  adb("logcat", "-c");
  tap("Update");
  const start = await until("the install intent", () => adb("logcat", "-d", "-s", "ActivityTaskManager:I").split("\n").find((l) => l.includes("application/vnd.android.package-archive")), 30_000);
  check("Update hands the APK to the installer", /act=android\.intent\.action\.VIEW/.test(start) && start.includes(`dat=content://${PKG}.files/`), start.trim().slice(0, 200));
  const got = adb("exec-out", "run-as", PKG, "stat", "-c", "%s", "cache/updates/yogurt-android-99.0.0.apk").trim();
  check("the downloaded APK is whole", Number(got) === fs.statSync(APK).size, `${got} bytes`);
  await sleep(2_000);
  const top = sh("dumpsys activity activities").match(/topResumedActivity=.*/)?.[0] ?? "";
  check("the package installer is in front", /packageinstaller/i.test(top), top.trim());
  sh("input keyevent KEYCODE_BACK");
}

main()
  .catch((e) => {
    console.log(`FAIL ${e.message}`);
    failures++;
  })
  .finally(async () => {
    await cloud.kill().catch(() => {});
    console.log(failures ? `${failures} check(s) failed` : "all checks passed");
    process.exit(failures ? 1 : 0);
  });
