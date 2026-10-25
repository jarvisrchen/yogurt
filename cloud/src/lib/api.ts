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

export class TokenRejected extends Error {}

export const OFFLINE = "Couldn't reach your Cloud. Check your connection and try again.";

/** postgrest-js reports a failed fetch as status 0 and auth-js as an AuthRetryableFetchError, both with the browser's raw message. */
export const offline = (e: { status?: number; name?: string }) => e.status === 0 || e.name === "AuthRetryableFetchError";

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
    if (error) throw offline(error) ? new Error(OFFLINE) : error;
  }

  /** Resolves true when the project hands back a session at once, which means email confirmation is off. */
  async signUp(email: string, password: string): Promise<boolean> {
    const { data, error } = await this.sb.auth.signUp({ email, password, options: { emailRedirectTo: location.origin } });
    if (error) throw offline(error) ? new Error(OFFLINE) : error;
    return !!data.session;
  }

  signOut = () => this.sb.auth.signOut();

  status = () => this.rpc<Status>("yogurt_status");
  setPasscode = (newPasscode: string, oldPasscode?: string) =>
    this.rpc<boolean>("yogurt_set_passcode", { new_passcode: newPasscode, old_passcode: oldPasscode ?? null });
  unlock = (passcode: string, remember: boolean, label: string) =>
    this.rpc<string | null>("yogurt_unlock", { passcode, remember, label });
  lock = (token: string) => this.rpc<boolean>("yogurt_lock", { token });
  browsers = (token: string) => this.rpc<Browser[]>("yogurt_browsers", { token });
  forgetBrowser = (token: string, browserId: number) =>
    this.rpc<boolean>("yogurt_forget_browser", { token, browser_id: browserId });
  devices = (token: string) => this.rpc<Device[]>("yogurt_device_list", { token });
}
