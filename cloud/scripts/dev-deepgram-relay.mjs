// Dev only, for `just cloud-dev`: never referenced by production code or Vercel config. It stands in for Deepgram's grant, which a key with only account:write cannot mint, and forwards the listen socket to Deepgram with the key in a header the browser never sees.
// Usage: YOGURT_DEEPGRAM_API_KEY=... node cloud/scripts/dev-deepgram-relay.mjs <port>
import { createServer } from "node:http";
import WebSocket, { WebSocketServer } from "ws";

const port = Number(process.argv[2] ?? 56449);
const key = process.env.YOGURT_DEEPGRAM_API_KEY;
if (!key) {
  console.error("dev-deepgram-relay: set YOGURT_DEEPGRAM_API_KEY");
  process.exit(1);
}
const UPSTREAM = "wss://api.deepgram.com";

// The browser offers the `bearer` subprotocol with its token, and the handshake fails unless the server picks one.
const wss = new WebSocketServer({ noServer: true, handleProtocols: (p) => (p.has("bearer") ? "bearer" : false) });

const server = createServer((req, res) => {
  if (req.method === "POST" && req.url === "/v1/auth/grant") {
    res.writeHead(200, { "content-type": "application/json" });
    return res.end(JSON.stringify({ access_token: "dev-relay", expires_in: 30 }));
  }
  res.writeHead(404).end();
});

server.on("upgrade", (req, sock, head) => {
  const url = new URL(req.url ?? "/", "http://relay");
  if (url.pathname !== "/v1/listen") return sock.end("HTTP/1.1 404 Not Found\r\n\r\n");
  wss.handleUpgrade(req, sock, head, (client) => relay(client, url.search));
});

/** Close codes a peer may send, since 1005 and 1006 only report that none came. */
const sendable = (code) => (code === 1000 || (code >= 3000 && code < 5000) ? code : 1000);

function relay(client, search) {
  const up = new WebSocket(`${UPSTREAM}/v1/listen${search}`, { headers: { authorization: `Token ${key}` } });
  const held = [];
  client.on("message", (data, binary) => {
    if (up.readyState === WebSocket.OPEN) up.send(data, { binary });
    else held.push([data, binary]);
  });
  up.on("open", () => {
    for (const [data, binary] of held.splice(0)) up.send(data, { binary });
  });
  up.on("message", (data, binary) => client.readyState === WebSocket.OPEN && client.send(data, { binary }));
  up.on("unexpected-response", (_req, res) => {
    console.error(`dev-deepgram-relay: Deepgram answered ${res.statusCode}`);
    client.close(1011, `Deepgram answered ${res.statusCode}`);
  });
  up.on("error", (e) => {
    console.error(`dev-deepgram-relay: ${e.message}`);
    client.close(1011);
  });
  up.on("close", (code) => client.close(sendable(code)));
  client.on("close", (code) => (up.readyState === WebSocket.OPEN ? up.close(sendable(code)) : up.terminate()));
}

server.listen(port, "127.0.0.1", () => console.log(`dev-deepgram-relay: http://localhost:${port} -> ${UPSTREAM}`));
