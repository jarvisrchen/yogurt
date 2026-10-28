import { expect, test } from "vitest";
import type { ChatMessage } from "./api";
import { historyTurns } from "./citations";

const at = (iso: string) => new Date(iso).getTime();
const sources = [
  { id: "a", title: "1:1 with Steven", started_at: at("2026-10-01T10:00") },
  { id: "b", title: "1:1 with Steven", started_at: at("2026-10-08T10:00") },
  { id: "c", title: " ", started_at: at("2026-10-09T10:00") },
];
const turn = (role: ChatMessage["role"], content: string, extra: Partial<ChatMessage> = {}): ChatMessage => ({ id: content, meeting_id: "", role, content, created_at: 1, ...extra });

test("the next question numbers its own meetings, so earlier answers lose their numbers and a typed one names its meeting", () => {
  expect(
    historyTurns([
      turn("user", "What did Steven say [1]?"),
      turn("assistant", "Ship it [1]. Then slip it [2, 3].", { sources }),
      turn("user", "What else came up in [2]? And in [2, 3], or [4]?"),
    ]),
  ).toEqual([
    { role: "user", content: "What did Steven say [1]?" },
    { role: "assistant", content: "Ship it. Then slip it." },
    { role: "user", content: "What else came up in (1:1 with Steven, 2026-10-08)? And in (1:1 with Steven, 2026-10-08; Untitled meeting, 2026-10-09), or [4]?" },
  ]);
});

test("a meeting chat's turns, which have no sources, pass through", () => {
  expect(historyTurns([turn("assistant", "At [1] they agreed.")])).toEqual([{ role: "assistant", content: "At [1] they agreed." }]);
});
