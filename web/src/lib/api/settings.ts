/**
 * Typed fetch wrappers + TypeScript types mirroring `crates/yogurt-server/src/api/settings.rs`.
 *
 * **Security invariant (mirrors the Rust side):** `ProviderView.api_key_masked`
 * is the ONLY key-derived value the SPA ever sees. There is no raw-key
 * surface — the `setProviderKey` mutation accepts a plaintext value and
 * posts it directly to `POST /api/settings/providers/:id/key`, then the
 * Settings page re-renders from the refetched (masked) shape. The raw
 * key is never held in React state after the mutation resolves.
 *
 * Phase 5 Plan 05-03.
 */

// ─── Types ──────────────────────────────────────────────────────────────────

export interface General {
  port: number;
  open_browser_on_start: boolean;
  audio_input_device: string;
}

export interface ProviderView {
  id: string;
  name: string;
  base_url: string;
  model: string;
  is_active: boolean;
  created_at: number;
  /** "••••XXXX" if a key is stored, null otherwise. Never the raw key. */
  api_key_masked: string | null;
}

export interface Preset {
  name: string;
  base_url: string;
  default_model: string;
}

export interface SettingsView {
  general: General;
  providers: ProviderView[];
  presets: Preset[];
}

export interface NewProvider {
  name: string;
  base_url: string;
  model: string;
}

export interface UpdateProvider {
  name: string;
  base_url: string;
  model: string;
}

/** Phase 2 audio devices shape — re-exported here so the Audio section
 *  (plan 05-04) can consume the same typed client. */
export interface AudioDevice {
  id: string;
  name: string;
  is_default: boolean;
}

// ─── HTTP helper ────────────────────────────────────────────────────────────

/**
 * Thin `fetch` wrapper that throws on non-2xx, returns `undefined` for 204
 * (the canonical no-body response from `set_provider_key` /
 * `delete_provider`), and otherwise parses JSON.
 *
 * Always sets `content-type: application/json` — the Rust handlers reject
 * untagged bodies via axum's `Json<T>` extractor.
 */
async function http<T>(input: string, init?: RequestInit): Promise<T> {
  const res = await fetch(input, {
    ...init,
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
  });
  if (!res.ok) {
    const body = await res.text();
    throw new Error(`${res.status} ${res.statusText}: ${body}`);
  }
  if (res.status === 204) return undefined as unknown as T;
  return res.json() as Promise<T>;
}

// ─── Settings API ────────────────────────────────────────────────────────────

export const settingsApi = {
  get: () => http<SettingsView>("/api/settings"),
  patch: (patch: Partial<General>) =>
    http<General>("/api/settings", {
      method: "PATCH",
      body: JSON.stringify(patch),
    }),
  createProvider: (p: NewProvider) =>
    http<ProviderView>("/api/settings/providers", {
      method: "POST",
      body: JSON.stringify(p),
    }),
  updateProvider: (id: string, p: UpdateProvider) =>
    http<ProviderView>(`/api/settings/providers/${id}`, {
      method: "PATCH",
      body: JSON.stringify(p),
    }),
  deleteProvider: (id: string) =>
    http<void>(`/api/settings/providers/${id}`, { method: "DELETE" }),
  activateProvider: (id: string) =>
    http<ProviderView>(`/api/settings/providers/${id}/activate`, {
      method: "POST",
    }),
  setProviderKey: (id: string, api_key: string) =>
    http<void>(`/api/settings/providers/${id}/key`, {
      method: "POST",
      body: JSON.stringify({ api_key }),
    }),
};

// ─── Audio API (Phase 2 endpoint, re-exported for plan 05-04 Audio section)
//
// The `/api/audio/devices` endpoint requires a session token (WR-08); the
// Settings page Audio section will need to attach the bootstrap token —
// that wiring lands in plan 05-04 when the Audio section is built.

export const audioApi = {
  devices: () => http<AudioDevice[]>("/api/audio/devices"),
};
