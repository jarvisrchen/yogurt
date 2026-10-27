/// <reference types="vitest/config" />
import { fileURLToPath } from "node:url";
import type { IncomingMessage, ServerResponse } from "node:http";
import { defineConfig, type Plugin, type UserConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { POST } from "./api/chat";

const web = fileURLToPath(new URL("../web", import.meta.url));
const here = (p: string) => fileURLToPath(new URL(p, import.meta.url));

// Vercel serves api/chat.ts in production. Here it runs under `vite` and `vite preview`, so the e2e specs reach the same handler.
function chatApi(): Plugin {
  const serve = async (req: IncomingMessage, res: ServerResponse) => {
    const chunks: Buffer[] = [];
    for await (const c of req) chunks.push(c as Buffer);
    const out = await POST(new Request("http://localhost/api/chat", { method: req.method, headers: req.headers as Record<string, string>, body: req.method === "POST" ? Buffer.concat(chunks) : undefined }));
    res.writeHead(out.status, Object.fromEntries(out.headers));
    if (!out.body) return res.end();
    const reader = out.body.getReader();
    res.on("close", () => void reader.cancel());
    for (let r = await reader.read(); !r.done; r = await reader.read()) res.write(r.value);
    res.end();
  };
  // A hook that returns a function has it run later as a post hook, so this returns nothing.
  const mount = (server: { middlewares: { use: (path: string, fn: (req: IncomingMessage, res: ServerResponse) => void) => void } }) => {
    // An unhandled rejection here would exit the whole server, which the e2e specs run against.
    server.middlewares.use("/api/chat", (req, res) =>
      void serve(req, res).catch(() => {
        if (!res.headersSent) res.statusCode = 500;
        res.end();
      }),
    );
  };
  return { name: "yogurt-chat-api", configureServer: mount, configurePreviewServer: mount };
}

// Vitest ships vite 5 peer types but we run vite 6, so the cast keeps tsc happy while Vitest still reads `test:`.
export default defineConfig({
  plugins: [react(), tailwindcss(), chatApi()],
  // The favicon and app icons are the desktop app's own files.
  publicDir: `${web}/public`,
  resolve: {
    alias: {
      "@web": `${web}/src`,
      // web/src/index.css does `@import "tailwindcss"`. Vercel installs only cloud/node_modules, so pin it there.
      tailwindcss: here("node_modules/tailwindcss/index.css"),
    },
    // Imports in ../web/src resolve from web/node_modules, which Vercel never installs, and react would bundle twice.
    dedupe: ["react", "react-dom", "markdown-it", "dompurify"],
  },
  // build.mjs clears dist and writes config.json before Vite runs, so Vite must keep it.
  build: { outDir: "dist", emptyOutDir: false },
  test: {
    globals: true,
    exclude: ["**/node_modules/**", "**/dist/**", "e2e/**"],
  },
} as UserConfig);
