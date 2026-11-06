// Types only, so the phone webapp can use ModelPicker without the Mac's query and API modules.

export type ModelName =
  | "tiny.en"
  | "small.en"
  | "medium.en"
  | "large-v3-turbo"
  | "large-v3";

export interface ModelView {
  name: ModelName;
  size_mb: number;
  downloaded: boolean;
  downloading: boolean;
  intel_supported: boolean;
  /** The verified copy lives in a Homebrew prefix. DELETE refuses these, so the picker shows a "brew" chip. */
  managed_by_homebrew: boolean;
}

export interface DownloadState {
  bytesDownloaded: number;
  totalBytes: number;
  bytesPerSec: number;
  etaSeconds: number | null;
  complete: boolean;
  error: string | null;
}
