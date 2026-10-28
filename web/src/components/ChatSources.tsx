import { Fragment, type ReactNode } from "react";
import type { ChatMessage as Msg } from "../lib/api";
import { splitTitle } from "./library/titles";

/**
 * The meetings a finished cross-meeting answer cites as `[n]`, keeping their numbers, as chips that open them.
 * `link` wraps each chip, so the app and the Cloud webapp each route it their own way.
 */
export function ChatSources({ message, link }: { message: Msg; link: (id: string, className: string, children: ReactNode) => ReactNode }) {
  const cited = (message.sources ?? [])
    .map((s, i) => ({ ...s, n: i + 1, ...splitTitle(s.title, s.started_at) }))
    .filter((s) => message.content.includes(`[${s.n}]`));
  if (cited.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-1.5 mt-2">
      {cited.map((s) => (
        <Fragment key={s.id}>
          {link(
            s.id,
            "inline-flex items-center gap-1.5 max-w-full h-6 px-2 rounded-chip text-[12px] text-[var(--color-ink)] bg-[var(--color-blsoft)] hover:bg-[var(--color-blue)] hover:text-white transition-colors",
            <>
              <span className="font-mono opacity-70">{s.n}</span>
              <span className="truncate">{s.name}</span>
              <span className="opacity-70">{s.date}</span>
            </>,
          )}
        </Fragment>
      ))}
    </div>
  );
}
