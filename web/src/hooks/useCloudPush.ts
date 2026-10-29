// The Yogurt Cloud push lives in this module, not in a component, so leaving Settings does not drop it.
// The Shell's one `/ws` subscription feeds it (see `useModelDownloadEvents`), and a reconnect re-reads the server's status.
import { useSyncExternalStore } from "react";
import type { PushStatus } from "../lib/api/cloud";
import { hideFinishedPill } from "./hideFinishedPill";

export type PushState =
  | { kind: "running"; sent: number; total: number; retryAt: number | null; leftSecs: number | null }
  | { kind: "done"; pushed: number; deleted: number }
  | { kind: "failed"; error: string };

export interface CloudPush {
  state: PushState;
  /** Set once a finished pill is clicked, closed, or times out, so it goes away while Settings still shows the result. */
  pillHidden: boolean;
}

export type CloudPushEvent =
  | { type: "cloud_push_progress"; sent: number; total: number; retry_in: number | null; left_secs: number | null }
  | { type: "cloud_push_complete"; pushed: number; deleted: number }
  | { type: "cloud_push_error"; error: string; code: string }
  | { type: "cloud_push_cancelled" };

let current: CloudPush | null = null;
const listeners = new Set<() => void>();

function set(next: CloudPush | null) {
  current = next;
  listeners.forEach((l) => l());
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function useCloudPush(): CloudPush | null {
  return useSyncExternalStore(subscribe, () => current);
}

const show = (state: PushState) => {
  const next = { state, pillHidden: false };
  set(next);
  return next;
};
const running = (
  sent: number,
  total: number,
  retryIn: number | null,
  leftSecs: number | null,
  now: number,
): PushState => ({
  kind: "running",
  sent,
  total,
  retryAt: retryIn == null ? null : now + retryIn * 1000,
  leftSecs,
});

export function applyCloudPushEvent(ev: CloudPushEvent, now = Date.now()) {
  switch (ev.type) {
    case "cloud_push_progress":
      show(running(ev.sent, ev.total, ev.retry_in, ev.left_secs, now));
      break;
    case "cloud_push_complete": {
      const shown = show({ kind: "done", pushed: ev.pushed, deleted: ev.deleted });
      hideFinishedPill(() => current === shown, hideCloudPushPill);
      break;
    }
    case "cloud_push_error":
      show({ kind: "failed", error: ev.error });
      break;
    case "cloud_push_cancelled":
      set(null);
      break;
  }
}

/**
 * A frame can be missed while the socket is down, and a page load misses the whole run so far. Ask the server instead.
 * A pass the server never announced (one batch) gets no pill, and a pass that ended while the socket was down just clears.
 */
export function applyCloudPushStatus(push: PushStatus | null, now = Date.now()) {
  const kind = current?.state.kind;
  if (push?.announced) {
    if (kind === undefined || kind === "running") show(running(push.sent, push.total, push.retry_in, push.left_secs, now));
  } else if (kind === "running") {
    set(null);
  }
}

export function hideCloudPushPill() {
  if (current) set({ ...current, pillHidden: true });
}

export function clearCloudPush() {
  set(null);
}

/** The one live view of a running push, so every surface reads the same numbers as the pill. The polled status fills in until the first frame lands. */
export function useLivePush(polled: PushStatus | null): PushStatus | null {
  const live = useCloudPush();
  if (!polled) return null;
  if (live?.state.kind !== "running") return polled;
  const { sent, total, leftSecs } = live.state;
  return { ...polled, sent, total, left_secs: leftSecs };
}
