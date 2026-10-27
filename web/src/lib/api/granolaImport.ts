import { bearerFetch } from "../session";

export interface ImportCounts {
  done: number;
  total: number;
  imported: number;
  skipped: number;
  failed: number;
}

export interface ImportLast extends ImportCounts {
  outcome: "complete" | "cancelled" | "failed";
  error: string | null;
  failed_titles: string[];
}

export interface ImportStatus {
  /** Masked form only. The server never returns the key. */
  key: string | null;
  running: boolean;
  progress: ImportCounts | null;
  last: ImportLast | null;
}

export interface ImportPreview {
  total: number;
  already_imported: number;
  to_import: number;
  captured_by_others: number;
}

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await bearerFetch(path, {
    method,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const text = await res.text();
  if (!res.ok) {
    let msg = `${res.status} ${res.statusText}`;
    try {
      const e = (JSON.parse(text) as { error?: unknown }).error;
      if (typeof e === "string") msg = e;
    } catch {
      // Not JSON: the status line is all there is to say.
    }
    throw new Error(msg);
  }
  return (text ? JSON.parse(text) : undefined) as T;
}

export const importApi = {
  status: () => call<ImportStatus>("GET", "/api/import/granola"),
  setKey: (key: string) => call<{ key: string | null }>("PUT", "/api/import/granola/key", { key }),
  deleteKey: () => call<void>("DELETE", "/api/import/granola/key"),
  preview: () => call<ImportPreview>("POST", "/api/import/granola/preview"),
  start: () => call<void>("POST", "/api/import/granola"),
  cancel: () => call<void>("DELETE", "/api/import/granola"),
};

export const IMPORT_KEY = ["granola-import"] as const;
