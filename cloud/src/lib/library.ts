import type { StoredTranscriptSegment } from "@web/lib/ws";
import type { LibraryRow, SearchHit } from "./api";

export interface Entry {
  key: string;
  device_id: string;
  pushId: number;
  pushedAt: string;
  title: string;
  /** Unix ms. Named like the desktop's meeting so the shared day grouping takes it as is. */
  started_at: number;
  endedAt: number | null;
  notes: string;
  enhanced: string | null;
  segments: StoredTranscriptSegment[];
}

export type Tab = "notes" | "enhanced" | "transcript";

export function parseSegments(json: string): StoredTranscriptSegment[] {
  try {
    const parsed: unknown = JSON.parse(json);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter(
      (s): s is StoredTranscriptSegment =>
        s !== null && typeof s === "object" && typeof s.ts_ms === "number" && (s.channel === "me" || s.channel === "them") && typeof s.text === "string",
    );
  } catch {
    return [];
  }
}

export const keyOf = (r: { device_id: string; meeting_id: string }) => `${r.device_id}/${r.meeting_id}`;
const titleOf = (title: string) => title.trim() || "Untitled meeting";

/** Newest first, like the desktop library. */
export function toEntries(rows: LibraryRow[]): Entry[] {
  return rows
    .map((r): Entry => {
      const m = r.body.meeting;
      return {
        key: keyOf(r),
        device_id: r.device_id,
        pushId: r.push_id,
        pushedAt: r.pushed_at,
        title: titleOf(m.title),
        started_at: m.started_at,
        endedAt: m.ended_at,
        notes: m.notes_md ?? "",
        enhanced: m.enriched_md,
        segments: parseSegments(m.transcript_json ?? "[]"),
      };
    })
    .sort((a, b) => b.started_at - a.started_at);
}

/** A search hit as an entry without content, for its list row or for a meeting the loaded library lacks. */
export const hitEntry = (h: SearchHit): Entry => ({
  key: keyOf(h),
  device_id: h.device_id,
  pushId: h.push_id,
  pushedAt: "",
  title: titleOf(h.title),
  started_at: h.started_at,
  endedAt: h.ended_at,
  notes: "",
  enhanced: null,
  segments: [],
});

export const forDevice = <T extends { device_id: string }>(items: T[], id: string | null): T[] => (id ? items.filter((i) => i.device_id === id) : items);

/** Tabs a meeting has content for, in display order. A meeting with none still gets Notes so the page is never blank. */
export function tabsFor(e: Entry): Tab[] {
  const tabs: Tab[] = [];
  if (e.notes.trim()) tabs.push("notes");
  if (e.enhanced?.trim()) tabs.push("enhanced");
  if (e.segments.length) tabs.push("transcript");
  return tabs.length ? tabs : ["notes"];
}

export function defaultTab(e: Entry): Tab {
  const tabs = tabsFor(e);
  return tabs.includes("enhanced") ? "enhanced" : tabs[0];
}

export function historyLabel(changed: number, deleted: number): string {
  const n = (count: number, verb: string) => `${count} ${count === 1 ? "meeting" : "meetings"} ${verb}`;
  if (changed && deleted) return `${n(changed, "updated")}, ${deleted} removed`;
  if (deleted) return n(deleted, "removed");
  if (changed) return n(changed, "updated");
  return "No changes";
}

export const clock = (ms: number | string) => new Date(ms).toLocaleTimeString("en-US", { hour: "numeric", minute: "2-digit" });
const shortDate = (ms: number | string) => new Date(ms).toLocaleDateString("en-US", { month: "numeric", day: "numeric", year: "2-digit" });

/** "10/22/26 · 1:42 PM · 22 min". A meeting still live, or one with no end, has no duration. */
export function started(e: Pick<Entry, "started_at" | "endedAt">): string {
  const min = duration(e);
  return `${shortDate(e.started_at)} · ${clock(e.started_at)}${min ? ` · ${min}` : ""}`;
}

export function duration(e: Pick<Entry, "started_at" | "endedAt">): string | null {
  if (e.endedAt == null || e.endedAt <= e.started_at) return null;
  return `${Math.max(1, Math.round((e.endedAt - e.started_at) / 60_000))} min`;
}

export const pushedFrom = (device: string, pushedAt: string) => `Pushed from ${device} · ${shortDate(pushedAt)} ${clock(pushedAt)}`;

export const asOfLabel = (pushedAt: string) =>
  new Date(pushedAt).toLocaleString("en-US", { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
