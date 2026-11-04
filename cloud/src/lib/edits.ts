import type { EditOp, PendingEdit } from "./api";
import { keyOf } from "./library";

/** A note as it lands at the end of a meeting's notes, stamped so it reads as added later. The Mac appends this text as is. */
export function noteBlock(text: string, now = new Date()): string {
  const when = now.toLocaleString("en-US", { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
  return `**Added in Yogurt Cloud, ${when}**\n\n${text.trim()}`;
}

/** Notes with `block` appended, as the Mac's `append_note` does. */
export const appendNote = (notes: string, block: string) => (notes.trim() ? `${notes.trimEnd()}\n\n${block}` : block);

/** How a meeting reads once its pending edits apply, in the order they were asked. */
export interface Overlay {
  title: string | null;
  deleted: boolean;
  notes: string[];
}

export function overlays(edits: PendingEdit[]): Map<string, Overlay> {
  const out = new Map<string, Overlay>();
  for (const e of edits) {
    const key = keyOf(e);
    const o = out.get(key) ?? { title: null, deleted: false, notes: [] };
    if (e.op === "rename") o.title = e.value;
    else if (e.op === "delete") o.deleted = true;
    else o.notes.push(e.value);
    out.set(key, o);
  }
  return out;
}

const WAITING: Record<EditOp, (e: PendingEdit) => string> = {
  rename: (e) => `rename this to “${e.value}”`,
  append_note: () => "add your note",
  delete: () => "delete this meeting",
};

export const waitingText = (e: PendingEdit, mac: string) => `Waiting for ${mac} to ${WAITING[e.op](e)}.`;
