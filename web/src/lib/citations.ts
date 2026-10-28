import type { ChatMessage, ChatRole, ChatSource } from "./api";

const CITE = /\s*\[(\d+(?:\s*,\s*\d+)*)\]/g;

/** A meeting by title and local date, as the MEETINGS list names it, so two meetings that share a title stay apart. */
function name(s: ChatSource): string {
  const d = new Date(s.started_at);
  const date = Number.isNaN(d.getTime()) ? "" : `, ${d.toLocaleDateString("en-CA")}`;
  return `${s.title.trim() || "Untitled meeting"}${date}`;
}

/**
 * The conversation as the model reads it on the next question, which finds and numbers its own meetings.
 * An earlier answer's `[n]` goes, so the model never reads an old number against the new list.
 * Replacing it with the meeting's title instead made MiniMax-M3 stop citing in 3 of 8 follow-ups.
 * A `[n]` the user typed becomes the meeting it meant in the answer before it.
 */
export function historyTurns(messages: ChatMessage[]): { role: ChatRole; content: string }[] {
  let sources: ChatSource[] | undefined;
  return messages.map(({ role, content, sources: own }) => {
    if (role === "assistant") {
      sources = own;
      return { role, content: own ? content.replace(CITE, "") : content };
    }
    const known = sources;
    if (!known) return { role, content };
    return {
      role,
      content: content.replace(CITE, (cite, list: string) => {
        const named = list.split(",").map((n) => known[Number(n) - 1]);
        return named.every(Boolean) ? ` (${named.map((s) => name(s!)).join("; ")})` : cite;
      }),
    };
  });
}
