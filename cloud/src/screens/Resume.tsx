import { Button } from "@web/components/Button";
import { Note } from "../ui";
import { clock } from "../lib/library";
import { UNTITLED, type Unfinished } from "../lib/phone";
import { useSession, type RecordingSession } from "../lib/session";

// Lucide's mic, as lucide-react draws it.
export const MicIcon = ({ size }: { size: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    <path d="M12 19v3" />
    <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
    <rect x="9" y="2" width="6" height="13" rx="3" />
  </svg>
);

/** The Library's card for a meeting a closed page left unfinished. */
export function ResumeCard({ session, unfinished, onResume, onEnded }: { session: RecordingSession; unfinished: Unfinished; onResume?: () => void; onEnded?: (key: string) => void }) {
  const snap = useSession(session);
  const title = unfinished.meeting.title.trim() || UNTITLED;
  return (
    <section className="rounded-card bg-card border border-line p-[14px] flex flex-col gap-[10px]">
      <div className="flex items-center gap-[8px] min-w-0">
        <span aria-hidden className="inline-block w-2 h-2 rounded-pill bg-straw flex-none" />
        <b className="text-[16px] min-[721px]:text-[13px] font-bold text-ink truncate">{title} stopped recording</b>
      </div>
      <p className="text-[15px] min-[721px]:text-[12.5px] text-mut m-[0px]">Yogurt closed at {clock(unfinished.saved_at)}. Your notes and the transcript up to then are saved.</p>
      {snap.error && <Note>{snap.error}</Note>}
      <div className="flex gap-[8px]">
        <Button
          onClick={() => {
            onResume?.();
            void session.resume();
          }}
          disabled={snap.ending}
          className="flex-1 !py-[12px] !text-[15px] min-[721px]:!py-[8px] min-[721px]:!text-[12.5px]"
        >
          Resume
        </Button>
        <Button
          variant="secondary"
          onClick={async () => {
            const key = await session.endUnfinished();
            if (key) onEnded?.(key);
          }}
          disabled={snap.ending}
          className="flex-1 !py-[12px] !text-[15px] min-[721px]:!py-[8px] min-[721px]:!text-[12.5px]"
        >
          {snap.ending ? "Saving…" : "End"}
        </Button>
      </div>
    </section>
  );
}
