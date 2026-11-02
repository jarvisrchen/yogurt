import { createServer, type Server } from "node:http";
import { WebSocketServer, type WebSocket } from "ws";

/** What one listen socket saw: its token and how much audio arrived, and when. */
export interface Socket {
  token: string;
  query: URLSearchParams;
  bytes: number;
  openedAt: number;
  /** Bytes received in the first 300 ms, which a replay of queued audio fills at once. */
  burst: number;
  loud: boolean;
}

/**
 * A stand-in for Deepgram at the address playwright.config.ts gives the stt-token function.
 * The grant checks the key. A socket must offer the bearer subprotocol with a granted token.
 * For every 2 seconds of audio a socket receives it sends an interim, then a final line, numbered across sockets, timed in that socket's audio as Deepgram does.
 */
export class FakeDeepgram {
  grants = 0;
  sockets: Socket[] = [];
  private refuseUntil = 0;
  private line = 0;
  private server!: Server;
  private live = new Set<WebSocket>();

  constructor(private readonly port: number, private readonly key: string) {}

  async start() {
    const wss = new WebSocketServer({ noServer: true, handleProtocols: (p) => (p.has("bearer") ? "bearer" : false) });
    this.server = createServer((req, res) => {
      if (req.method === "POST" && req.url === "/v1/auth/grant" && req.headers.authorization === `Token ${this.key}` && Date.now() >= this.refuseUntil) {
        this.grants++;
        res.writeHead(200, { "content-type": "application/json" });
        return res.end(JSON.stringify({ access_token: `granted-${this.grants}`, expires_in: 30 }));
      }
      res.writeHead(req.url === "/v1/auth/grant" ? 403 : 404).end();
    });
    this.server.on("upgrade", (req, sock, head) => {
      const [kind, token] = String(req.headers["sec-websocket-protocol"] ?? "").split(",").map((s) => s.trim());
      const url = new URL(req.url ?? "/", "http://x");
      if (url.pathname !== "/v1/listen" || kind !== "bearer" || !token?.startsWith("granted-") || Date.now() < this.refuseUntil) {
        sock.end("HTTP/1.1 401 Unauthorized\r\n\r\n");
        return;
      }
      wss.handleUpgrade(req, sock, head, (ws) => this.listen(ws, { token, query: url.searchParams, bytes: 0, openedAt: Date.now(), burst: 0, loud: false }));
    });
    await new Promise<void>((r) => this.server.listen(this.port, "127.0.0.1", r));
  }

  private listen(ws: WebSocket, s: Socket) {
    this.sockets.push(s);
    this.live.add(ws);
    ws.on("close", () => this.live.delete(ws));
    let sent = 0;
    const result = (text: string, start: number, final: boolean) =>
      ws.send(JSON.stringify({ type: "Results", channel: { alternatives: [{ transcript: text, confidence: 0.9 }] }, start, duration: 1, is_final: final, speech_final: final }));
    ws.on("message", (data, binary) => {
      if (!binary) {
        if (JSON.parse(String(data)).type === "CloseStream") ws.close(1000);
        return;
      }
      const buf = data as Buffer;
      s.bytes += buf.length;
      if (Date.now() - s.openedAt < 300) s.burst += buf.length;
      for (let i = 0; i + 1 < buf.length; i += 2) if (Math.abs(buf.readInt16LE(i)) > 500) s.loud = true;
      // One step per second of audio: an interim on odd seconds, the final on even ones.
      for (; sent < Math.floor(s.bytes / 32_000); sent++) {
        const start = sent - (sent % 2);
        if (sent % 2 === 0) result(`Heard line ${this.line + 1}`, start, false);
        else result(`Heard line ${++this.line} in full.`, start, true);
      }
    });
  }

  /** Closes every socket and refuses grants and sockets for `ms`, as a network drop does. */
  drop(ms: number) {
    this.refuseUntil = Date.now() + ms;
    for (const ws of this.live) ws.terminate();
  }

  stop() {
    for (const ws of this.live) ws.terminate();
    return new Promise<void>((r) => this.server.close(() => r()));
  }
}
