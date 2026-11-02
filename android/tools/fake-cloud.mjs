// A fake Yogurt Cloud, Deepgram, and GitHub release for the Android app's end-to-end test (e2e.mjs). Node only, no dependencies.
//
// GET  /                       a test page that drives window.YogurtAndroid the way the webapp's NativeRecorder does
// GET  /cmd, POST /report      the driver's channel to that page: one queued command out, the page's state back
// POST /cmd                    queues a command by hand, as in curl -d '{"cmd":"start","meetingId":"m-1"}' localhost:8787/cmd
// POST /api/stt-token          checks the session like cloud/api/stt-token.ts and returns {token, url}
// WS   /v1/listen              checks the granted token, then turns each 2 s of audio into one final line
// GET  /releases/latest        a release with yogurt-android-99.0.0.apk, served from the APK given to fakeCloud
//
// The test wav stamps every sample with its second (see e2e.mjs), so a line's text says which audio it came from:
// a lost or doubled line after a reconnect shows up as a missing or repeated "second N".

import crypto from "node:crypto";
import fs from "node:fs";
import http from "node:http";

const BLOCK = 2 * 16_000 * 2; // 2 s of 16 kHz linear16
const INTERIM = BLOCK / 4;

export function fakeCloud({ port = 8787, apk = null, log = () => {} } = {}) {
  const state = { tokens: 0, sockets: 0, finals: 0, cmds: [], page: null, reports: 0 };
  let server = null;
  const conns = new Set();

  function handle(req, res) {
    const url = new URL(req.url, `http://localhost:${port}`);
    const body = () => new Promise((done) => {
      let s = "";
      req.on("data", (c) => (s += c));
      req.on("end", () => done(s));
    });
    const json = (status, o) => {
      res.writeHead(status, { "content-type": "application/json" });
      res.end(JSON.stringify(o));
    };
    if (req.method === "GET" && url.pathname === "/") {
      res.writeHead(200, { "content-type": "text/html" });
      return res.end(PAGE);
    }
    if (url.pathname === "/cmd" && req.method === "POST") {
      return body().then((s) => {
        state.cmds.push(JSON.parse(s));
        json(200, {});
      });
    }
    if (url.pathname === "/cmd") return json(200, state.cmds.shift() ?? {});
    if (url.pathname === "/report") {
      return body().then((s) => {
        state.page = JSON.parse(s);
        state.reports++;
        json(200, {});
      });
    }
    if (url.pathname === "/api/stt-token") {
      return body().then((s) => {
        let token = null;
        try {
          ({ token } = JSON.parse(s));
        } catch {}
        if (req.headers.authorization !== "Bearer jwt-test") return json(401, { message: "Sign in again." });
        if (token !== "browser-test") return json(400, { message: "The request has no browser token." });
        state.tokens++;
        log(`token ${state.tokens}`);
        json(200, { token: `tok-${state.tokens}`, url: `ws://localhost:${port}/v1/listen?model=nova-3&encoding=linear16&sample_rate=16000&channels=1` });
      });
    }
    if (url.pathname === "/releases/latest") {
      const name = "yogurt-android-99.0.0.apk";
      return json(200, { tag_name: "v9.9.9", assets: [{ name, browser_download_url: `http://localhost:${port}/${name}` }] });
    }
    if (url.pathname === "/yogurt-android-99.0.0.apk" && apk) {
      res.writeHead(200, { "content-type": "application/vnd.android.package-archive" });
      return fs.createReadStream(apk).pipe(res);
    }
    json(404, { message: "Not found." });
  }

  function upgrade(req, socket) {
    conns.add(socket);
    socket.on("error", () => {});
    socket.on("close", () => conns.delete(socket));
    if (!req.url.startsWith("/v1/listen") || !/^Bearer tok-\d+$/.test(req.headers.authorization ?? "")) {
      log(`listen refused: ${req.headers.authorization}`);
      return socket.end("HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\n\r\n");
    }
    const accept = crypto.createHash("sha1").update(req.headers["sec-websocket-key"] + "258EAFA5-E914-47DA-95CA-C5AB0DC85B11").digest("base64");
    socket.write(`HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ${accept}\r\n\r\n`);
    const id = ++state.sockets;
    log(`socket ${id} open (${req.headers.authorization})`);

    let raw = Buffer.alloc(0);
    let audio = Buffer.alloc(0); // this socket's audio not yet turned into a final
    let base = 0; // bytes of this socket's audio before audio[0]
    let interims = 0;
    const send = (op, payload) => {
      if (socket.writableEnded || socket.destroyed) return;
      const len = payload.length;
      const head = len < 126 ? Buffer.from([0x80 | op, len]) : Buffer.from([0x80 | op, 126, len >> 8, len & 255]);
      socket.write(Buffer.concat([head, payload]));
    };
    const result = (bytes, final) => {
      const second = audio.readInt16LE(0);
      const text = final ? `second ${second}` : `second ${second} so far`;
      send(1, Buffer.from(JSON.stringify({
        type: "Results",
        channel: { alternatives: [{ transcript: text }] },
        is_final: final,
        speech_final: final,
        start: base / 32_000,
        duration: bytes / 32_000,
      })));
      if (final) state.finals++;
    };
    const onAudio = (buf) => {
      audio = Buffer.concat([audio, buf]);
      while (audio.length >= BLOCK) {
        result(BLOCK, true);
        audio = audio.subarray(BLOCK);
        base += BLOCK;
        interims = 0;
      }
      if (audio.length >= (interims + 1) * INTERIM) {
        interims++;
        result(audio.length, false);
      }
    };
    socket.on("data", (chunk) => {
      raw = Buffer.concat([raw, chunk]);
      for (;;) {
        if (raw.length < 2) return;
        const op = raw[0] & 15;
        let len = raw[1] & 127;
        let at = 2;
        if (len === 126) {
          if (raw.length < 4) return;
          len = raw.readUInt16BE(2);
          at = 4;
        } else if (len === 127) {
          if (raw.length < 10) return;
          len = Number(raw.readBigUInt64BE(2));
          at = 10;
        }
        if (raw.length < at + 4 + len) return;
        const mask = raw.subarray(at, at + 4);
        const payload = Buffer.from(raw.subarray(at + 4, at + 4 + len));
        for (let i = 0; i < len; i++) payload[i] ^= mask[i & 3];
        raw = raw.subarray(at + 4 + len);
        if (op === 2) onAudio(payload);
        else if (op === 9) send(10, payload);
        else if (op === 1 && JSON.parse(payload).type === "CloseStream") {
          // Like Deepgram: finish what was heard, then close.
          if (audio.length >= 2) result(audio.length, true);
          log(`socket ${id} CloseStream`);
          send(8, Buffer.from([0x03, 0xe8]));
          socket.end();
        } else if (op === 8) {
          send(8, Buffer.from([0x03, 0xe8]));
          socket.end();
        }
      }
    });
  }

  return {
    state,
    url: `http://localhost:${port}`,
    command: (c) => state.cmds.push(c),
    listen: () => new Promise((done) => {
      server = http.createServer(handle);
      server.on("upgrade", upgrade);
      server.on("connection", (s) => {
        conns.add(s);
        s.on("close", () => conns.delete(s));
      });
      server.listen(port, done);
    }),
    // Like a killed process: every connection drops with no close frame.
    kill: () => new Promise((done) => {
      for (const s of conns) s.destroy();
      server.close(() => done());
    }),
  };
}

const PAGE = `<!doctype html>
<html><head><meta name="viewport" content="width=device-width, initial-scale=1"><title>Bridge test</title>
<style>
body { font: 15px sans-serif; margin: 16px; background: #FBF7EF; color: #211D18; overflow-wrap: anywhere; }
@media (prefers-color-scheme: dark) { body { background: #1B1815; color: #EDEAE2; } }
</style></head>
<body>
<h3>Bridge test</h3>
<div id="status"></div>
<ol id="lines" start="0"></ol>
<script>
const B = window.YogurtAndroid;
// The page's own copy of the meeting and its lines, as the webapp keeps its meeting in IndexedDB.
let meetingId = localStorage.getItem("meetingId") || "";
let lines = JSON.parse(localStorage.getItem("lines") || "[]");
const events = [];
const pageId = Math.random();
let stopping = false;
const save = () => {
  localStorage.setItem("meetingId", meetingId);
  localStorage.setItem("lines", JSON.stringify(lines));
};
function catchUp(why) {
  const from = lines.length;
  const got = JSON.parse(B.lines(meetingId, from));
  lines.push(...got);
  save();
  events.push({ type: "catchup", why, from, got: got.length });
}
function report() {
  const status = JSON.parse(B.status());
  document.getElementById("status").textContent = JSON.stringify(status);
  document.getElementById("lines").start = Math.max(0, lines.length - 8);
  document.getElementById("lines").innerHTML = lines.slice(-8).map((l) => "<li>" + l.text + "</li>").join("");
  fetch("/report", { method: "POST", body: JSON.stringify({ pageId, meetingId, status, lines, events: events.slice(-200), version: B.appVersion() }) }).catch(() => {});
}
addEventListener("yogurt-native", (e) => {
  const d = e.detail;
  if (d.type !== "interim") {
    events.push(d.type === "line" ? { type: "line", meetingId: d.meetingId, index: d.index } : d);
    console.log("yogurt-native " + JSON.stringify(d));
  }
  if (d.type === "state" && d.state === "stopped" && stopping) {
    // As the record screen does: the lines it missed, once the app says the last ones went out.
    stopping = false;
    catchUp("stop");
    events.push({ type: "stopped-call", status: JSON.parse(B.status()) });
  }
  if (d.type === "line" && d.meetingId === meetingId) {
    if (d.index === lines.length) {
      lines.push(d.line);
      save();
    } else if (d.index > lines.length) catchUp("skip");
  }
  if (d.type !== "interim") report();
});
document.addEventListener("visibilitychange", () => {
  if (document.visibilityState === "visible") {
    catchUp("visible");
    report();
  }
});
async function poll() {
  try {
    const c = await (await fetch("/cmd")).json();
    if (c.cmd === "start") {
      // Starting the meeting the page holds resumes it, with the lines it already has.
      if (c.meetingId !== meetingId) lines = [];
      meetingId = c.meetingId;
      save();
      const auth = { jwt: "jwt-test", browserToken: "browser-test" };
      const r = B.start(JSON.stringify({ meetingId, startedAtMs: Date.now(), have: lines.length, tokenUrl: "/api/stt-token", auth }));
      events.push({ type: "start", meetingId, have: lines.length, result: JSON.parse(r) });
      catchUp("start");
    }
    if (c.cmd === "start-raw") events.push({ type: "start", result: JSON.parse(B.start(c.json)) });
    if (c.cmd === "stop") {
      const t = Date.now();
      stopping = true;
      B.stop();
      events.push({ type: "stop-returned", ms: Date.now() - t });
    }
    if (c.cmd === "keep") {
      lines = lines.slice(0, c.n);
      save();
    }
    if (c.cmd === "reload") return location.reload();
    if (c.cmd) report();
  } catch {}
  setTimeout(poll, 300);
}
catchUp("load");
report();
poll();
</script>
</body></html>`;

if (import.meta.url === `file://${process.argv[1]}`) {
  const port = Number(process.argv[2] ?? 8787);
  const cloud = fakeCloud({ port, apk: process.argv[3] ?? null, log: (m) => console.log(m) });
  await cloud.listen();
  console.log(`fake Cloud on ${cloud.url}`);
}
