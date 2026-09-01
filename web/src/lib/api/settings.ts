/**
 * Typed fetch wrappers + TypeScript types mirroring `crates/yogurt-server/src/api/settings.rs`.
 *
 * **Security invariant (mirrors the Rust side):** `ProviderView.api_key_masked`
 * is the ONLY key-derived value the SPA ever sees. There is no raw-key
 * surface - the `setProviderKey` mutation accepts a plaintext value and
 * posts it directly to `POST /api/settings/providers/:id/key`, then the
 * Settings page re-renders from the refetched (masked) shape. The raw
 * key is never held in React state after the mutation resolves.
 *
 * Phase 5 Plan 05-03.
 */

// ─── Types ──────────────────────────────────────────────────────────────────

/**
 * Result of `POST /api/settings/providers/:id/test` - one live round-trip
 * against the provider.
 *
 * A rejected key still comes back HTTP 200 with `ok: false`; the request
 * succeeded, the answer is just "no". Non-200 means the test could not be
 * run at all (unknown id).
 */
export interface TestConnectionResult {
  ok: boolean;
  /** Model the provider echoed back, which may differ from the one asked for. */
  model?: string;
  error?: string;
}

export interface General {
  port: number;
  open_browser_on_start: boolean;
  audio_input_device: string;
  /** Phase 7 - `true` once the user finishes `/welcome`. Drives the
   *  first-run redirect (`useFirstRunRedirect`). */
  first_run_completed: boolean;
  /** Phase 8 (Plan 08-03) - `"cloud"` (Deepgram) or `"local"` (WhisperLocal).
   *  Mirrors `crates/yogurt-db::settings::General::stt_provider`. */
  stt_provider: string;
  /** Phase 8 (Plan 08-03) - one of the names in `yogurt_stt::models::REGISTRY`
   *  (e.g. `"tiny.en"`, `"small.en"`, `"medium.en"`, `"large-v3"`).
   *  Mirrors `crates/yogurt-db::settings::General::stt_model`. */
  stt_model: string;
  /** MTG-11 - watch for meeting-app windows and offer to start recording.
   *  Detection never starts a recording on its own; it only surfaces the
   *  prompt (and stops a recording once the detected window closes).
   *  Mirrors `crates/yogurt-db::settings::General::meeting_detection`. */
  meeting_detection: boolean;
}

/**
 * `"http"` (`OpenAiCompatClient` against `base_url` + a stored API key) or
 * `"cli"` (LLM-4: a local agent CLI, `yogurt_llm::CliClient` - no base URL
 * or key involved; `model` holds the CLI program id "claude" |
 * "cursor-agent" | "opencode" instead of a model name). Mirrors
 * `yogurt_db::providers::adapter`.
 */
export type ProviderAdapter = "http" | "cli";

export interface ProviderView {
  id: string;
  name: string;
  base_url: string;
  model: string;
  is_active: boolean;
  created_at: number;
  /** "••••XXXX" if a key is stored, null otherwise. Never the raw key.
   *  Always null for a `cli`-adapter provider - it never has a key. */
  api_key_masked: string | null;
  adapter: ProviderAdapter;
  /** `cli`-adapter only: the `--model` value passed to the CLI. Empty
   *  means "use the CLI's own default". Meaningless for `http`. */
  cli_model: string;
}

export interface Preset {
  name: string;
  base_url: string;
  default_model: string;
  /**
   * Static list of popular model ids for this preset, used to seed the
   * MODEL `<datalist>` on a freshly-cloned `http` provider, or the
   * `--model` picker's suggestions on a `cli` provider. The Settings
   * page's `Refresh` button replaces this with the live catalog - the
   * `/v1/models` response once a key is on file for `http` providers,
   * or the binary's own listing for a `cli` one (LLM-6). Empty for
   * runtimes like Ollama / LM Studio where the model list is purely
   * local; one entry for a `cli` preset whose real catalog is too large
   * or too account-specific to hardcode (Cursor Agent, OpenCode).
   */
  models: string[];
  /**
   * Public URL of the provider's model catalog. Rendered as a small
   * `See all models →` link next to the MODEL field so users have a
   * discovery surface for preview / regional tiers the static list
   * doesn't cover, and a fallback when `/v1/models` isn't supported.
   * Empty for `adapter: "cli"` presets - there is no catalog page.
   */
  docs_url: string;
  adapter: ProviderAdapter;
  /** `cli`-adapter only: default `--model` suggestion (e.g. "haiku",
   *  or a fully qualified "minimax-coding-plan/MiniMax-M3" for
   *  opencode), seeded into a freshly-cloned provider's `cli_model`.
   *  Empty for `http` presets. */
  default_cli_model: string;
}

export interface SettingsView {
  general: General;
  providers: ProviderView[];
  presets: Preset[];
  /** "••••XXXX" when a Deepgram STT key is stored, null otherwise. */
  deepgram_key_masked: string | null;
}

export interface NewProvider {
  name: string;
  base_url: string;
  model: string;
  /** Defaults to `"http"` server-side if omitted - only `PresetChip`
   *  sends `"cli"`, and only for the two built-in local-CLI presets. */
  adapter?: ProviderAdapter;
  /** `cli`-adapter only: the `--model` value to seed on the new row.
   *  Defaults to `""` server-side if omitted. */
  cli_model?: string;
}

export interface UpdateProvider {
  name: string;
  base_url: string;
  model: string;
  /** `cli`-adapter only: the `--model` value to persist. Defaults to
   *  `""` server-side if omitted; `http` rows should send the current
   *  value back unchanged. */
  cli_model?: string;
}

/** Phase 2 audio devices shape - re-exported here so the Audio section
 *  (plan 05-04) can consume the same typed client. Mirrors the backend
 *  `DeviceInfo` struct (`crates/yogurt-audio/src/mic.rs`) - there is no
 *  `id` field, only `name` (the identifier the backend matches on). */
export interface AudioDevice {
  name: string;
  is_default: boolean;
  sample_rate?: number | null;
}

// ─── HTTP helper ────────────────────────────────────────────────────────────

/**
 * Thin `fetch` wrapper that attaches the bootstrap session token (Phase 0
 * WR-06), throws on non-2xx, returns `undefined` for 204 (the canonical
 * no-body response from `set_provider_key` / `delete_provider`), and
 * otherwise parses JSON.
 *
 * Always sets `content-type: application/json` - the Rust handlers reject
 * untagged bodies via axum's `Json<T>` extractor.
 *
 * On a non-2xx response the backend returns `{ "error": "…" }` for 422
 * (validation) and 502 (upstream provider failure) - if the body parses
 * as JSON with a string `error` field, that string becomes the thrown
 * message so the UI can show it directly instead of a raw status line.
 * Anything else falls back to `${status} ${statusText}: ${body}`.
 */
async function http<T>(input: string, init?: RequestInit): Promise<T> {
  const res = await bearerFetch(input, {
    ...init,
    headers: { "content-type": "application/json", ...(init?.headers ?? {}) },
  });
  if (!res.ok) {
    const body = await res.text();
    let parsedError: string | undefined;
    try {
      const parsed = JSON.parse(body);
      if (parsed && typeof parsed.error === "string") parsedError = parsed.error;
    } catch {
      // Not JSON - fall through to the raw status line below.
    }
    throw new Error(parsedError ?? `${res.status} ${res.statusText}: ${body}`);
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
  /**
   * `POST /api/settings/providers/:id/test` - verify a key actually works
   * before committing it. Pass the draft key to test something you have not
   * saved yet; omit it to test whatever is already stored.
   *
   * The draft never reaches the key file, and the server scrubs it out of
   * the provider's error text before replying.
   */
  testProvider: (id: string, api_key?: string) =>
    http<TestConnectionResult>(`/api/settings/providers/${id}/test`, {
      method: "POST",
      body: JSON.stringify(api_key ? { api_key } : {}),
    }),
  /** `POST /api/settings/stt/key` - stores the Deepgram STT key in the
   *  key file. No provider id: the STT key is a singleton, keyed server-side
   *  by `DEEPGRAM_KEY_ID`. */
  setSttKey: (api_key: string) =>
    http<void>("/api/settings/stt/key", {
      method: "POST",
      body: JSON.stringify({ api_key }),
    }),
  /**
   * `POST /api/settings/stt/test` - verify the Deepgram key actually works
   * before committing it. Pass the draft key to test something you have not
   * saved yet; omit it to test whatever is already stored. No `model` field
   * in the response - STT has nothing to echo back.
   */
  testSttKey: (api_key?: string) =>
    http<TestConnectionResult>("/api/settings/stt/test", {
      method: "POST",
      body: JSON.stringify(api_key ? { api_key } : {}),
    }),
  /**
   * `POST /api/settings/providers/:id/models` - probe the provider's
   * `/v1/models` and return the list of model ids it advertises. A draft
   * `apiKey` is preferred over the stored key so the user can
   * discover what's available *before* saving - useful when the saved
   * model is the only thing wrong (e.g. Google's frequent deprecations).
   * The draft never reaches the key file. With neither a draft nor a
   * stored key the probe goes out unauthenticated - local runtimes
   * (Ollama, LM Studio) answer anyway; a provider that needs a key 401s,
   * which surfaces as a 502 `{ error }` and renders inline.
   */
  listProviderModels: (id: string, apiKey?: string) =>
    http<string[]>(`/api/settings/providers/${id}/models`, {
      method: "POST",
      body: JSON.stringify(apiKey ? { api_key: apiKey } : {}),
    }),
};

// ─── Audio API (Phase 2 endpoint, re-exported for plan 05-04 Audio section)
//
// The `/api/audio/devices` endpoint requires a session token (WR-08); the
// Settings page Audio section will need to attach the bootstrap token -
// that wiring lands in plan 05-04 when the Audio section is built.

export const audioApi = {
  devices: () => http<AudioDevice[]>("/api/audio/devices"),
  /** `POST /api/meetings/:id/audio-device` (quick task 260709-wnn) - hot-swap
   *  the mic device on an actively-recording meeting. Returns the resolved
   *  device name so the caller can reflect the actual active device. */
  switchMeetingDevice: (meetingId: string, deviceId: string) =>
    http<{ status: string; device: string }>(
      `/api/meetings/${meetingId}/audio-device`,
      {
        method: "POST",
        body: JSON.stringify({ device_id: deviceId }),
      },
    ),
  /** `POST /api/meetings/:id/mic-muted` (AUD-6) - pause/resume the mic on an
   *  actively-recording meeting. `Channel::System` is untouched. */
  setMicMuted: (meetingId: string, muted: boolean) =>
    http<{ status: string; muted: boolean }>(
      `/api/meetings/${meetingId}/mic-muted`,
      {
        method: "POST",
        body: JSON.stringify({ muted }),
      },
    ),
};

// ─── React-Query hooks (Phase 7 onboarding + permission gating) ─────────────
//
// `useSettings` is a thin shared cache so the Sidebar's `Local-only` pill,
// the Welcome route's CTA gate, and `useFirstRunRedirect` all share one
// network round-trip. `useSetFirstRunCompleted` flips the onboarding flag
// when the user clicks "Take me to my meetings →" - it `PATCH`es and
// invalidates the cache so the redirect hook re-resolves to `/`.

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { bearerFetch } from "../session";

/** Shared cache key - Sidebar, Welcome, and useFirstRunRedirect all read it. */
export const settingsKey = ["settings"] as const;

/** Cached `GET /api/settings`. 30s staleTime matches the Sidebar usage. */
export function useSettings(): UseQueryResult<SettingsView, Error> {
  return useQuery({
    queryKey: settingsKey,
    queryFn: () => settingsApi.get(),
    staleTime: 30_000,
  });
}

/**
 * `PATCH /api/settings` with `{ first_run_completed: value }`.
 *
 * Returns the updated `General` (the backend re-loads after the upsert so
 * the response always reflects the persisted state). Invalidates the
 * shared `settings` cache so any consumer of `useSettings()` - including
 * `useFirstRunRedirect` - picks up the new value immediately.
 */
export function useSetFirstRunCompleted(): UseMutationResult<
  General,
  Error,
  boolean
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (value: boolean) =>
      settingsApi.patch({ first_run_completed: value } as Partial<General>),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: settingsKey });
    },
  });
}
