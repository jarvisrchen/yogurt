import { describe, expect, it } from "vitest";
import type { PendingEdit } from "./api";
import { appendNote, noteBlock, overlays, waitingText } from "./edits";

const edit = (id: number, meeting_id: string, op: PendingEdit["op"], value = ""): PendingEdit => ({ id, device_id: "mac", meeting_id, op, value, created_at: "" });

describe("edits", () => {
  it("stamps a note and appends it after a blank line, or alone on empty notes", () => {
    const block = noteBlock("  Call Steven \n", new Date(2026, 10, 4, 10, 42));
    expect(block).toBe("**Added in Yogurt Cloud, Nov 4, 10:42 AM**\n\nCall Steven");
    expect(appendNote("Agenda\n\n", block)).toBe(`Agenda\n\n${block}`);
    expect(appendNote("  \n", block)).toBe(block);
  });

  it("folds pending edits per meeting, the last rename winning", () => {
    const o = overlays([edit(1, "a", "rename", "One"), edit(2, "a", "append_note", "n1"), edit(3, "a", "rename", "Two"), edit(4, "b", "delete")]);
    expect(o.get("mac/a")).toEqual({ title: "Two", deleted: false, notes: ["n1"] });
    expect(o.get("mac/b")).toEqual({ title: null, deleted: true, notes: [] });
    expect(o.has("mac/c")).toBe(false);
  });

  it("says what each edit waits for", () => {
    expect(waitingText(edit(1, "a", "rename", "Q4"), "Desk Mac")).toBe("Waiting for Desk Mac to rename this to “Q4”.");
    expect(waitingText(edit(2, "a", "delete"), "Desk Mac")).toBe("Waiting for Desk Mac to delete this meeting.");
  });
});
