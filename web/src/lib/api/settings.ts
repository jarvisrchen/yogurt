// `ProviderView.api_key_masked` is the only key-derived value the SPA sees. `setProviderKey` posts a
// plaintext key once and the raw key is never held in React state afterwards.

/**
 * A rejected key still returns HTTP 200 with `ok: false`. Non-200 means the test could not run (unknown id).
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
  first_run_completed: boolean;
  /** `"cloud"` (Deepgram) or `"local"` (WhisperLocal). */
  stt_provider: string;
  /** One of the names in `yogurt_stt::models::REGISTRY`. */
  stt_model: string;
  /** Watch for meeting-app windows and offer to start recording. Detection never starts a recording by
   *  itself, and stops one once the detected window closes. */
  meeting_detection: boolean;
  /** Raise the installed app on detection. Only acts while
   *  `meeting_detection` is also on. */
  meeting_detection_focus: boolean;
  /** Stop the recording when the meeting window closes or both channels stay
   *  silent for 5 minutes. */
  meeting_auto_stop: boolean;
  /** Whether mic echo is exposed at all. Off by default; the in-meeting echo controls and auto-start
   *  only exist when on. */
  audio_echo_feature: boolean;
  /** Output device the mic is echoed to, "" means system default. */
  audio_echo_output_device: string;
  /** Whether mic-echo starts enabled on the next recording. */
  audio_echo_enabled: boolean;
  audio_echo_buffer: number;
}

/**
 * `"cli"` is a local agent CLI (`yogurt_llm::CliClient`) with no base URL or key. `model` then holds
 * the CLI program id ("claude" | "cursor-agent" | "opencode").
 */
export type ProviderAdapter = "http" | "cli";

export interface ProviderView {
  id: string;
  name: string;
  base_url: string;
  model: string;
  is_active: boolean;
  created_at: number;
  /** "••••XXXX" if a key is stored, null otherwise. Never the raw key, and always null for `cli`. */
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
   * Popular model ids that seed the MODEL `<datalist>` or the `--model` suggestions. The Settings
   * `Refresh` button replaces them with the live catalog. Empty for local runtimes (Ollama, LM Studio),
   * and one entry for `cli` presets whose catalog is too large to hardcode.
   */
  models: string[];
  /** Model catalog URL for the "See all models" link, a fallback when `/v1/models` is unsupported. Empty for `cli`. */
  docs_url: string;
  adapter: ProviderAdapter;
  /** `cli` only: default `--model` suggestion (e.g. "haiku"), seeded into a freshly-cloned provider's `cli_model`. */
  default_cli_model: string;
}

export interface SettingsView {
  general: General;
  providers: ProviderView[];
  presets: Preset[];
  /** "••••XXXX" when a Deepgram STT key is stored, null otherwise. */
  deepgram_key_masked: string | null;
  /** Where this server keeps its files, with the home directory shown as `~`. */
  paths: DataPaths;
}

export interface DataPaths {
  data_dir: string;
  keys: string;
  notes: string;
}

export interface NewProvider {
  name: string;
  base_url: string;
  model: string;
  /** Defaults to `"http"` server-side. Only `PresetChip` sends `"cli"`. */
  adapter?: ProviderAdapter;
  /** `cli` only: the `--model` value to seed. */
  cli_model?: string;
}

export interface UpdateProvider {
  name: string;
  base_url: string;
  model: string;
  /** `cli` only. `http` rows should send the current value back unchanged. */
  cli_model?: string;
}

/** There is no `id` field: the backend matches on `name`. */
export interface AudioDevice {
  name: string;
  is_default: boolean;
  sample_rate?: number | null;
}

/**
 * Throws on non-2xx, preferring the backend's `{ "error": "…" }` message (422 validation, 502 upstream)
 * so the UI can show it directly. Returns `undefined` for 204.
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
      // Not JSON: fall through to the raw status line below.
    }
    throw new Error(parsedError ?? `${res.status} ${res.statusText}: ${body}`);
  }
  if (res.status === 204) return undefined as unknown as T;
  return res.json() as Promise<T>;
}

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
   * Pass a draft key to test an unsaved one, or omit it to test the stored key. The draft never reaches
   * the key file, and the server scrubs it from the provider's error text.
   */
  testProvider: (id: string, api_key?: string) =>
    http<TestConnectionResult>(`/api/settings/providers/${id}/test`, {
      method: "POST",
      body: JSON.stringify(api_key ? { api_key } : {}),
    }),
  /** No provider id: the STT key is a singleton keyed server-side by `DEEPGRAM_KEY_ID`. */
  setSttKey: (api_key: string) =>
    http<void>("/api/settings/stt/key", {
      method: "POST",
      body: JSON.stringify({ api_key }),
    }),
  /** Pass a draft key or omit it to test the stored one. The response has no `model`, since STT has nothing to echo. */
  testSttKey: (api_key?: string) =>
    http<TestConnectionResult>("/api/settings/stt/test", {
      method: "POST",
      body: JSON.stringify(api_key ? { api_key } : {}),
    }),
  /** Loads the model and transcribes a silent clip. Omit `model` to test the selected `stt_model`. */
  testLocalStt: (model?: string) =>
    http<TestConnectionResult>("/api/settings/stt/local/test", {
      method: "POST",
      body: JSON.stringify(model ? { model } : {}),
    }),
  /**
   * A draft `apiKey` is preferred over the stored key so users can discover models before saving. With
   * neither, the probe goes out unauthenticated: local runtimes answer anyway and a provider that needs
   * a key 401s, surfacing as a 502 `{ error }`.
   */
  listProviderModels: (id: string, apiKey?: string) =>
    http<string[]>(`/api/settings/providers/${id}/models`, {
      method: "POST",
      body: JSON.stringify(apiKey ? { api_key: apiKey } : {}),
    }),
};

export const audioApi = {
  devices: () => http<AudioDevice[]>("/api/audio/devices"),
  outputDevices: () => http<AudioDevice[]>("/api/audio/output-devices"),
  /** Hot-swap the mic device on an actively-recording meeting. Returns the resolved device name. */
  switchMeetingDevice: (meetingId: string, deviceId: string) =>
    http<{ status: string; device: string }>(
      `/api/meetings/${meetingId}/audio-device`,
      {
        method: "POST",
        body: JSON.stringify({ device_id: deviceId }),
      },
    ),
  /** Pause/resume the mic on an actively-recording meeting. `Channel::System` is untouched. */
  setMicMuted: (meetingId: string, muted: boolean) =>
    http<{ status: string; muted: boolean }>(
      `/api/meetings/${meetingId}/mic-muted`,
      {
        method: "POST",
        body: JSON.stringify({ muted }),
      },
    ),
  setEcho: (meetingId: string, body: { enabled?: boolean; device?: string }) =>
    http<{ enabled: boolean; device: string }>(
      `/api/meetings/${meetingId}/echo`,
      {
        method: "POST",
        body: JSON.stringify(body),
      },
    ),
  /** Plays a 440 Hz tone on the echo output device. A missing or empty `device` falls back to the persisted one, then system default. */
  testEcho: (device?: string) =>
    http<TestConnectionResult>("/api/audio/echo/test", {
      method: "POST",
      body: JSON.stringify(device ? { device } : {}),
    }),
};

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { bearerFetch } from "../session";

export const settingsKey = ["settings"] as const;

export function useSettings(): UseQueryResult<SettingsView, Error> {
  return useQuery({
    queryKey: settingsKey,
    queryFn: () => settingsApi.get(),
    staleTime: 30_000,
  });
}

/** Undefined until settings load. */
export function useDataPaths(): DataPaths | undefined {
  return useSettings().data?.paths;
}

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
