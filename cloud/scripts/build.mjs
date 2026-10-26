// Vercel build: write dist/config.json, run Vite, then on production apply the schema and owner row.
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dbConfig } from "./db.mjs";

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
const env = process.env;
const fail = (msg) => {
  console.error(`yogurt-cloud build: ${msg}`);
  process.exit(1);
};

// The Supabase Vercel integration sets both, and a manual setup sets them by hand.
const supabaseUrl = env.SUPABASE_URL;
const publishableKey = env.SUPABASE_PUBLISHABLE_KEY;
if (!supabaseUrl) fail("set SUPABASE_URL.");
if (!publishableKey) fail("set SUPABASE_PUBLISHABLE_KEY.");

const production = env.VERCEL_ENV === "production";
const dbUrl = env.POSTGRES_URL_NON_POOLING;
const owner = env.YOGURT_OWNER_EMAIL?.trim();
if (production && !dbUrl) fail("production builds need POSTGRES_URL_NON_POOLING, which the Supabase integration provides.");
if (production && !owner) fail("production builds need YOGURT_OWNER_EMAIL, the email of the one Cloud login.");

const dist = here("../dist");
rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });
writeFileSync(`${dist}/config.json`, JSON.stringify({ v: 1, supabase_url: supabaseUrl, publishable_key: publishableKey }) + "\n");

const vite = spawnSync("pnpm", ["exec", "vite", "build"], { cwd: here(".."), stdio: "inherit" });
if (vite.status !== 0) process.exit(vite.status ?? 1);

// Migrate only after the build succeeded, so a broken build never changes the production schema.
if (production) {
  const { default: pg } = await import("pg");
  const client = new pg.Client(dbConfig(dbUrl));
  await client.connect();
  try {
    await client.query(readFileSync(here("../../crates/yogurt-cloud/schema.sql"), "utf8"));
    await client.query(readFileSync(here("../../crates/yogurt-cloud/owner.sql"), "utf8"), [owner]);
    console.log(`yogurt-cloud build: schema applied, owner ${owner}`);
  } finally {
    await client.end();
  }
}
