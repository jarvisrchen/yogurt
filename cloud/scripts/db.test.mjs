import { describe, expect, it } from "vitest";
import { dbConfig } from "./db.mjs";

describe("dbConfig", () => {
  it("strips ssl params and trusts the remote host without verification", () => {
    const c = dbConfig("postgres://u:p%40ss@db.abc.supabase.co:5432/postgres?sslmode=require&sslrootcert=x&application_name=y");
    expect(c.connectionString).toBe("postgres://u:p%40ss@db.abc.supabase.co:5432/postgres?application_name=y");
    expect(c.ssl).toEqual({ rejectUnauthorized: false });
  });
  it("turns TLS off for a local database", () => {
    expect(dbConfig("postgresql://postgres:postgres@127.0.0.1:56432/postgres").ssl).toBe(false);
    expect(dbConfig("postgresql://postgres:postgres@localhost:5432/postgres?sslmode=disable").ssl).toBe(false);
  });
});
