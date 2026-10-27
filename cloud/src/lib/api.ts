import { createClient, type SupabaseClient } from "@supabase/supabase-js";
import type { Status } from "./route";

export interface Config {
  v: 1;
  supabase_url: string;
  publishable_key: string;
}

export interface Browser {
  id: number;
  label: string;
  created_at: string;
  expires_at: string;
  last_used_at: string | null;
  current: boolean;
}

export interface Device {
  id: string;
  name: string;
  created_at: string;
}

/** One pushed meeting, `crates/yogurt-db` `bundle_value`. `transcript_json` is the desktop's stored text, a JSON array of segments. */
export interface Bundle {
  v: 1;
  meeting: {
    id: string;
    title: string;
    started_at: number;
    ended_at: number | null;
    notes_md: string;
    enriched_md: string | null;
    transcript_json: string;
  };
}

export interface LibraryRow {
  device_id: string;
  push_id: number;
  pushed_at: string;
  meeting_id: string;
  body: Bundle;
}

/** A meeting matching a search. `snippet` is null when the title matched, and otherwise text with the matches marked (see `snippetParts`). `more` is true when the server stopped at its cap. */
export interface SearchHit {
  device_id: string;
  meeting_id: string;
  push_id: number;
  started_at: number;
  ended_at: number | null;
  title: string;
  snippet: string | null;
  more: boolean;
}

export interface HistoryRow {
  push_id: number;
  device_id: string;
  device_name: string;
  pushed_at: string;
  changed: number;
  deleted: number;
}

export class TokenRejected extends Error {}

export const OFFLINE = "Couldn't reach your Cloud. Check your connection and try again.";

/** postgrest-js reports a failed fetch as status 0 and auth-js as an AuthRetryableFetchError, both with the browser's raw message. */
export const offline = (e: { status?: number; name?: string }) => e.status === 0 || e.name === "AuthRetryableFetchError";

export const SIGNUP_CLOSED = "This Cloud isn't taking new logins. Sign in with the owner account.";

/** GoTrue answers a closed project with `signup_disabled`, whose own message ("Signups not allowed for this instance") reads like a bug. */
export function authError(e: { status?: number; name?: string; code?: string }) {
  if (offline(e)) return new Error(OFFLINE);
  if (e.code === "signup_disabled") return new Error(SIGNUP_CLOSED);
  return e;
}

export async function loadConfig(): Promise<Config> {
  const res = await fetch("/config.json", { cache: "no-store" });
  if (!res.ok) throw new Error(`config.json answered ${res.status}`);
  const c = await res.json();
  if (c?.v !== 1 || !c.supabase_url || !c.publishable_key) throw new Error("config.json is not a v1 Yogurt Cloud config");
  return c;
}

export class Api {
  readonly sb: SupabaseClient;

  constructor(cfg: Config) {
    this.sb = createClient(cfg.supabase_url, cfg.publishable_key);
  }

  private async rpc<T>(fn: string, args: object = {}): Promise<T> {
    const { data, error, status } = await this.sb.rpc(fn, args);
    if (error) {
      if (offline({ status })) throw new Error(OFFLINE);
      if (error.code === "28000" || status === 403) throw new TokenRejected(error.message);
      throw Object.assign(new Error(error.message), { code: error.code });
    }
    return data as T;
  }

  async signIn(email: string, password: string) {
    const { error } = await this.sb.auth.signInWithPassword({ email, password });
    if (error) throw authError(error);
  }

  /** Resolves true when the project hands back a session at once, which means email confirmation is off. */
  async signUp(email: string, password: string): Promise<boolean> {
    const { data, error } = await this.sb.auth.signUp({ email, password, options: { emailRedirectTo: location.origin } });
    if (error) throw authError(error);
    return !!data.session;
  }

  signOut = () => this.sb.auth.signOut({ scope: "local" });

  status = () => this.rpc<Status>("yogurt_status");
  setPasscode = (newPasscode: string, oldPasscode?: string) =>
    this.rpc<boolean>("yogurt_set_passcode", { new_passcode: newPasscode, old_passcode: oldPasscode ?? null });
  unlock = (passcode: string, remember: boolean, label: string) =>
    this.rpc<string | null>("yogurt_unlock", { passcode, remember, label });
  lock = (token: string) => this.rpc<boolean>("yogurt_lock", { token });
  browsers = (token: string) => this.rpc<Browser[]>("yogurt_browsers", { token });
  forgetBrowser = (token: string, browserId: number) =>
    this.rpc<boolean>("yogurt_forget_browser", { token, browser_id: browserId });
  library = (token: string, atPush?: number) => this.rpc<LibraryRow[]>("yogurt_library", { token, at_push: atPush ?? null });
  search = (token: string, query: string, device: string | null) => this.rpc<SearchHit[]>("yogurt_search", { token, query, only_device: device });
  history = (token: string) => this.rpc<HistoryRow[]>("yogurt_history", { token });
  devices = (token: string) => this.rpc<Device[]>("yogurt_device_list", { token });
}
