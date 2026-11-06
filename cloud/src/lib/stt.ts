import { STT_ENGINE } from "./phone";

/** What transcribes a recording here: Deepgram, or [model] on the phone, which only the Android app can run. */
export interface SttChoice {
  engine: "cloud" | "local";
  /** The model Local uses, kept while Cloud is in use, as the Mac keeps its `stt_model`. */
  model: string;
}

/** A model the Android app can run, with its download as the app last reported it. */
export interface PhoneModel {
  name: string;
  size_mb: number;
  downloaded: boolean;
  downloading: boolean;
  bytes: number;
  total: number;
  /** Why a download is not moving, such as waiting for Wi-Fi. */
  waiting: string | null;
  error: string | null;
}

export const DEFAULT_MODEL = "parakeet-v3";
// Per phone, since the models live on the phone.
const KEY = "yogurt-cloud-stt";

export function readStt(): SttChoice {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "null");
    if (typeof v?.model === "string" && v.model) return { engine: v.engine === "local" ? "local" : "cloud", model: v.model };
  } catch {
    // Unreadable storage records with Cloud.
  }
  return { engine: "cloud", model: DEFAULT_MODEL };
}

export function saveStt(c: SttChoice): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(c));
  } catch {
    // Storage blocked: the next recording uses Cloud.
  }
}

/** The model the next recording runs on the phone, or null for Deepgram. A browser without the app always uses Deepgram. */
export const localChoice = (): string | null => {
  const c = readStt();
  return window.YogurtAndroid && c.engine === "local" ? c.model : null;
};

/** The meeting's `stt_engine`, which the Mac shows as its engine pill. */
export const engineFor = (model: string | null) => (model ? `local · ${model} · phone` : STT_ENGINE);

/** The model a meeting's `stt_engine` names when the phone transcribed it. */
export const localModel = (engine: string | null | undefined): string | null => (engine?.startsWith("local · ") ? engine.split(" · ")[1] : null);

export const phoneModels = (): PhoneModel[] => (window.YogurtAndroid ? JSON.parse(window.YogurtAndroid.models()) : []);
