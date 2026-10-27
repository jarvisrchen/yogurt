import { Link } from "react-router";
import type { ChatMessage as Msg } from "../lib/api";
import { splitTitle } from "./library/InlineTitle";

/** The meetings a finished cross-meeting answer cites as `[n]`, keeping their numbers, as chips that open them. */
export function ChatSources({ message }: { message: Msg }) {
  const cited = (message.sources ?? [])
    .map((s, i) => ({ ...s, n: i + 1, ...splitTitle(s.title, s.started_at) }))
    .filter((s) => message.content.includes(`[${s.n}]`));
  if (cited.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-1.5 mt-2">
      {cited.map((s) => (
        <Link
          key={s.id}
          to={`/meeting/${s.id}/post`}
          className="inline-flex items-center gap-1.5 max-w-full h-6 px-2 rounded-chip text-[12px] text-[var(--color-ink)] bg-[var(--color-blsoft)] hover:bg-[var(--color-blue)] hover:text-white transition-colors"
        >
          <span className="font-mono opacity-70">{s.n}</span>
          <span className="truncate">{s.name}</span>
          <span className="opacity-70">{s.date}</span>
        </Link>
      ))}
    </div>
  );
}
