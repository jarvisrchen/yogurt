// The Granola import lives in this module, not in a component, so leaving Settings does not drop it.
// The Shell's one `/ws` subscription feeds it (see `useModelDownloadEvents`), and a reconnect re-reads the server's status.
import { useSyncExternalStore } from "react";
import type { ImportCounts, ImportStatus } from "../lib/api/granolaImport";
import { hideFinishedPill } from "./hideFinishedPill";

export type ImportState =
  | { kind: "running"; progress: ImportCounts }
  | { kind: "done"; imported: number; skipped: number; failed: number; failedTitles: string[] }
  | { kind: "failed"; error: string }
  | { kind: "cancelled"; imported: number; skipped: number; failed: number };

export interface GranolaImport {
  state: ImportState;
  /** Set once a finished pill is clicked, closed, or times out, so it goes away while Settings still shows the result. */
  pillHidden: boolean;
}

export type ImportEvent =
  | ({ type: "import_progress" } & ImportCounts)
  | { type: "import_complete"; imported: number; skipped: number; failed: number; failed_titles: string[] }
  | { type: "import_error"; error: string }
  | { type: "import_cancelled"; imported: number; skipped: number; failed: number };

const ZERO: ImportCounts = { done: 0, total: 0, imported: 0, skipped: 0, failed: 0 };

let current: GranolaImport | null = null;
const listeners = new Set<() => void>();

function set(next: GranolaImport | null) {
  current = next;
  listeners.forEach((l) => l());
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function useGranolaImport(): GranolaImport | null {
  return useSyncExternalStore(subscribe, () => current);
}

const show = (state: ImportState) => {
  const next = { state, pillHidden: false };
  set(next);
  return next;
};

export function applyImportEvent(ev: ImportEvent) {
  switch (ev.type) {
    case "import_progress":
      show({ kind: "running", progress: ev });
      break;
    case "import_complete": {
      const shown = show({
        kind: "done",
        imported: ev.imported,
        skipped: ev.skipped,
        failed: ev.failed,
        failedTitles: ev.failed_titles,
      });
      // A run with failed meetings stays up, like a failed run, until the user closes it.
      if (ev.failed === 0) hideFinishedPill(() => current === shown, hideImportPill);
      break;
    }
    case "import_error":
      show({ kind: "failed", error: ev.error });
      break;
    case "import_cancelled":
      show({ kind: "cancelled", imported: ev.imported, skipped: ev.skipped, failed: ev.failed });
      break;
  }
}

/** The start request was accepted. The first progress frame may already have landed. */
export function markImportStarted() {
  if (current?.state.kind !== "running") show({ kind: "running", progress: ZERO });
}

/** A frame can be missed while the socket is down, and a page load misses the whole run so far. Ask the server instead. */
export function applyImportStatus(s: ImportStatus) {
  if (s.running) {
    if (s.progress) show({ kind: "running", progress: s.progress });
    else markImportStarted();
    return;
  }
  if (current?.state.kind !== "running" || !s.last) return;
  const l = s.last;
  if (l.outcome === "complete") {
    applyImportEvent({ type: "import_complete", ...l, failed_titles: l.failed_titles });
  } else if (l.outcome === "cancelled") {
    applyImportEvent({ type: "import_cancelled", ...l });
  } else {
    applyImportEvent({ type: "import_error", error: l.error ?? "The import stopped." });
  }
}

export function hideImportPill() {
  if (current) set({ ...current, pillHidden: true });
}

export function clearImport() {
  set(null);
}
