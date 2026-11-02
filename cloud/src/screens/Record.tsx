import { useEffect, useMemo, useRef, useState, type PointerEvent, type ReactNode } from "react";
import { Button } from "@web/components/Button";
import { TranscriptLine, formatTs } from "@web/components/TranscriptLine";
import { YogurtEditor } from "@web/editor";
import { Back, DeviceTag, Field, Note } from "../ui";
import type { Api } from "../lib/api";
import type { Gap } from "../lib/audioQueue";
import { clock } from "../lib/library";
import { readDevice, UNTITLED, type Line, type PhoneMeeting } from "../lib/phone";
import { QUEUE_MS } from "../lib/audioQueue";
import { useSession, type RecordingSession, type Snapshot } from "../lib/session";
import { MicIcon, ResumeCard } from "./Resume";

const dateTime = (ms: number) => `${new Date(ms).toLocaleDateString("en-US", { month: "short", day: "numeric" })} · ${clock(ms)}`;
const minSec = (ms: number) => {
  const s = Math.floor(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

function useNow(on: boolean) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    if (!on) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [on]);
  return now;
}

/** A device's color as the Library shows it, read once. */
function useDeviceColor(api: Api, token: string, id: string) {
  const [color, setColor] = useState("slate");
  useEffect(() => {
    api.devices(token).then((ds) => {
      const c = ds.find((d) => d.id === id)?.color;
      if (c) setColor(c);
    }, () => undefined);
  }, [api, token, id]);
  return color;
}

function Head({ title, onTitle, device, color, startedAt }: { title: string; onTitle: (t: string) => void; device: string; color: string; startedAt: number }) {
  return (
    <div>
      <input
        aria-label="Title"
        value={title}
        placeholder={UNTITLED}
        onChange={(e) => onTitle(e.target.value)}
        className="w-full bg-transparent border-0 p-[0px] outline-none font-[inherit] text-[22px] min-[721px]:text-[20px] font-extrabold tracking-[-0.01em] text-ink placeholder:text-grey m-[0px]"
      />
      <div className="flex flex-wrap items-center gap-[6px] min-[721px]:gap-[8px] text-[13px] min-[721px]:text-[12px] text-mut mt-[3px] min-[721px]:mt-[4px]">
        <DeviceTag name={device} color={color} />
        {dateTime(startedAt)}
      </div>
    </div>
  );
}

const NOTICE = "text-[13px] min-[721px]:text-[11.5px] rounded-button px-[12px] py-[8px] bg-blsoft text-ink m-[0px]";

interface Props {
  api: Api;
  token: string;
  session: RecordingSession;
  onBack: () => void;
  onEnded: (key: string) => void;
}

/** Ready to record, then recording, until End. While it records it is the whole page, with no way back but End. */
export function Record({ api, token, session, onBack, onEnded }: Props) {
  const snap = useSession(session);
  const device = useMemo(readDevice, [snap.meeting]);
  const color = useDeviceColor(api, token, device.id);
  if (snap.meeting) return <Recording snap={snap} meeting={snap.meeting} session={session} device={device.name} color={color} onEnded={onEnded} />;
  return <Ready snap={snap} session={session} device={device} color={color} onBack={onBack} />;
}

function Ready({ snap, session, device, color, onBack }: { snap: Snapshot; session: RecordingSession; device: { name: string; registered: boolean }; color: string; onBack: () => void }) {
  const [title, setTitle] = useState("");
  const [name, setName] = useState(device.name);
  const [busy, setBusy] = useState(false);
  const now = useNow(true);
  const start = async () => {
    setBusy(true);
    await session.start(title, name);
    setBusy(false);
  };
  return (
    <main className="min-h-dvh w-full max-w-[640px] mx-auto flex flex-col gap-[12px] px-[16px] pt-[16px] pb-[24px]">
      <div>
        <Back to="All devices" onClick={onBack} />
      </div>
      {snap.unfinished && <ResumeCard session={session} unfinished={snap.unfinished} />}
      <Head title={title} onTitle={setTitle} device={device.registered ? device.name : name.trim() || "Phone"} color={color} startedAt={now} />
      <section className="rounded-card bg-card border border-line p-[16px] flex flex-col gap-[10px]">
        <div className="w-[44px] h-[44px] rounded-[14px] grid place-items-center bg-blsoft text-blue">
          <MicIcon size={22} />
        </div>
        <h2 className="text-[18px] min-[721px]:text-[15px] font-bold tracking-[-0.01em] text-ink m-[0px]">Record the room</h2>
        <p className="text-[15px] min-[721px]:text-[12.5px] text-mut m-[0px]">The phone's microphone picks up everyone in the room. The audio streams to Deepgram to be transcribed and is never saved.</p>
        <p className="text-[15px] min-[721px]:text-[12.5px] text-mut m-[0px]">A call on this phone cannot be recorded. Take the call on another device, on speaker.</p>
      </section>
      {!device.registered && (
        <div className="flex flex-col gap-[5px]">
          <Field label="This phone's name" value={name} maxLength={80} onChange={(e) => setName(e.target.value)} />
          <span className="text-[13px] min-[721px]:text-[11.5px] text-mut">Its meetings show under this name in the Library.</span>
        </div>
      )}
      {snap.error && <Note>{snap.error}</Note>}
      <div className="mt-auto flex flex-col gap-[8px] pt-[12px]">
        <Button onClick={start} disabled={busy} className="w-full !py-[14px] !text-[16px] min-[721px]:!py-[11px] min-[721px]:!text-[13px]">
          <MicIcon size={18} />
          {busy ? "Starting…" : "Start recording"}
        </Button>
        <p className="text-[13px] min-[721px]:text-[11.5px] text-mut text-center m-[0px]">Your phone asks for the microphone first.</p>
      </div>
    </main>
  );
}

const NOTES_PLACEHOLDER = "Take sparse notes while the transcript runs below.";
const PEEK = 152;

function Recording({ snap, meeting, session, device, color, onEnded }: { snap: Snapshot; meeting: PhoneMeeting; session: RecordingSession; device: string; color: string; onEnded: (key: string) => void }) {
  const now = useNow(true);
  const [full, setFull] = useState(false);
  const end = async () => {
    const key = await session.end();
    if (key) onEnded(key);
  };
  const reconnecting = snap.state === "reconnecting" && snap.downSince !== null;
  // Interim words carry no time, so they take the moment they started showing.
  const interimAt = useRef<number | null>(null);
  if (!snap.interim) interimAt.current = null;
  else interimAt.current ??= now - meeting.started_at;
  return (
    <div className="h-dvh w-full max-w-[640px] mx-auto flex flex-col min-[721px]:border-x min-[721px]:border-line">
      <div className="flex-none flex items-center justify-between gap-[10px] px-[16px] py-[10px] border-b border-line bg-paper">
        <span role="status" className="flex items-center gap-[8px] min-w-0">
          <span aria-hidden className={`inline-block w-2 h-2 rounded-pill bg-straw ${snap.ending ? "" : "animate-recpulse"}`} />
          <span className="text-[15px] min-[721px]:text-[13px] font-semibold text-ink">{snap.ending ? "Saving" : "Recording"}</span>
          <span className="font-mono text-[14px] min-[721px]:text-[12px] tracking-tight text-mut">{minSec((meeting.ended_at ?? now) - meeting.started_at)}</span>
        </span>
        <Button onClick={end} disabled={snap.ending} className="!px-[14px] !py-[8px] !text-[15px] min-[721px]:!px-[12px] min-[721px]:!py-[5px] min-[721px]:!text-[12.5px] !font-bold !leading-normal">
          {snap.ending ? "Saving…" : snap.saveError && meeting.ended_at ? "Try again" : "End meeting"}
        </Button>
      </div>
      <div className="relative flex-1 min-h-0">
        <main className="absolute inset-0 overflow-y-auto flex flex-col gap-[10px] px-[16px] pt-[14px]" style={{ paddingBottom: PEEK + 16 }}>
          {reconnecting ? (
            <p role="status" className={NOTICE}>
              Reconnecting… Holding {minSec(Math.min(now - snap.downSince!, QUEUE_MS))} of audio, up to 5 minutes. It is transcribed once the connection is back.
            </p>
          ) : (
            !snap.native && <p className={NOTICE}>Keep this page open. Locking the phone or switching apps can stop the recording. The screen stays on while you record.</p>
          )}
          {snap.error && <Note>{snap.error}</Note>}
          {snap.saveError && <Note>Not saved to your Cloud yet. {snap.saveError}{meeting.ended_at ? "" : " It tries again every 30 seconds."}</Note>}
          <Head title={meeting.title} onTitle={(t) => session.setTitle(t)} device={device} color={color} startedAt={meeting.started_at} />
          <section className="cloud-notes rounded-card bg-card p-[16px] border border-line text-[16px] min-[721px]:text-[13px] leading-relaxed">
            <YogurtEditor initialMarkdown={meeting.notes_md} onChange={(md) => session.setNotes(md)} placeholder={NOTES_PLACEHOLDER} />
          </section>
        </main>
        <Sheet full={full} setFull={setFull} state={snap.state} lines={meeting.lines} interim={snap.interim} interimAt={interimAt.current ?? 0} gaps={snap.gaps} />
      </div>
    </div>
  );
}

const STATUS: Record<Snapshot["state"], ReactNode> = {
  live: (
    <span className="text-[11px] inline-flex items-center gap-1 whitespace-nowrap text-matcha">
      <span aria-hidden>●</span>connected
    </span>
  ),
  connecting: (
    <span className="text-[11px] inline-flex items-center gap-1 whitespace-nowrap text-grey">
      <span aria-hidden>○</span>connecting
    </span>
  ),
  reconnecting: (
    <span className="text-[11px] inline-flex items-center gap-1 whitespace-nowrap text-grey">
      <span aria-hidden>○</span>reconnecting
    </span>
  ),
  stopped: (
    <span className="text-[11px] inline-flex items-center gap-1 whitespace-nowrap text-grey">
      <span aria-hidden>○</span>stopped
    </span>
  ),
};

function GapMarker({ gap }: { gap: Gap }) {
  return (
    <div className="flex items-center gap-[8px] py-[6px] text-[13px] min-[721px]:text-[11.5px] text-mut" data-gap>
      <span className="flex-1 h-px bg-line" />
      <span className="whitespace-nowrap">
        Not transcribed <span className="font-mono text-[12px]">{formatTs(gap.from_ms)}</span> to <span className="font-mono text-[12px]">{formatTs(gap.to_ms)}</span>
      </span>
      <span className="flex-1 h-px bg-line" />
    </div>
  );
}

type Row = { line: Line } | { gap: Gap };
const rowTs = (r: Row) => ("line" in r ? r.line.ts_ms : r.gap.from_ms);

/** The live transcript: two rows peeking at the bottom, or pulled up to just under the recording bar. A tap on its header or a drag moves it. */
function Sheet({ full, setFull, state, lines, interim, interimAt, gaps }: { full: boolean; setFull: (f: boolean) => void; state: Snapshot["state"]; lines: Line[]; interim: string; interimAt: number; gaps: Gap[] }) {
  const rows = useMemo<Row[]>(() => [...lines.map((line) => ({ line })), ...gaps.map((gap) => ({ gap }))].sort((a, b) => rowTs(a) - rowTs(b)), [lines, gaps]);
  const shown = full ? rows : rows.slice(-2);
  const body = useRef<HTMLDivElement>(null);
  const pinned = useRef(true);
  useEffect(() => {
    const el = body.current;
    if (el && pinned.current) el.scrollTop = el.scrollHeight;
  }, [rows, interim, full]);
  const drag = useRef<number | null>(null);
  const down = (e: PointerEvent) => (drag.current = e.clientY);
  const up = (e: PointerEvent) => {
    if (drag.current === null) return;
    const dy = e.clientY - drag.current;
    drag.current = null;
    if (Math.abs(dy) < 8) setFull(!full);
    else setFull(dy < 0);
  };
  return (
    <section
      aria-label="Live transcript"
      className="absolute inset-x-0 bottom-0 z-40 flex flex-col bg-card border-t border-line rounded-t-panel"
      style={{ height: full ? "100%" : PEEK, boxShadow: "0 -16px 32px -8px var(--shadow-card-color)" }}
    >
      <div
        role="button"
        tabIndex={0}
        aria-expanded={full}
        aria-label={full ? "Lower transcript" : "Raise transcript"}
        onPointerDown={down}
        onPointerUp={up}
        onPointerCancel={() => (drag.current = null)}
        onKeyDown={(e) => (e.key === "Enter" || e.key === " ") && (e.preventDefault(), setFull(!full))}
        className="flex-none cursor-pointer select-none touch-none outline-none"
      >
        <div className="flex justify-center pt-[8px]">
          <span className="block w-[36px] h-[4px] rounded-full bg-line" />
        </div>
        <header className={`flex items-center justify-between gap-2 px-[20px] pt-[6px] ${full ? "border-b border-line pb-[10px]" : "pb-[4px]"}`}>
          <span className="text-[13px] min-[721px]:text-[12px] font-semibold text-ink">Live transcript</span>
          <span className="flex items-center gap-[8px]">
            {STATUS[state]}
            {full && (
              <span aria-hidden className="inline-flex px-[6px] py-[4px] text-mut">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="m6 9 6 6 6-6" />
                </svg>
              </span>
            )}
          </span>
        </header>
      </div>
      <div
        ref={body}
        onScroll={(e) => {
          const el = e.currentTarget;
          pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
        }}
        className={full ? "flex-1 min-h-0 overflow-y-auto px-[20px] pb-[28px] text-[16px] min-[721px]:text-[13px]" : "h-[76px] flex flex-col justify-end overflow-hidden px-[20px] text-[16px] min-[721px]:text-[13px] [mask-image:linear-gradient(to_bottom,transparent_0,#000_22px)]"}
      >
        {!rows.length && !interim && <p className="py-2 leading-snug text-grey m-[0px]">{state === "live" ? "Listening…" : "Connecting to Deepgram…"}</p>}
        {shown.map((r, i) => ("line" in r ? <TranscriptLine key={i} ev={{ ts_ms: r.line.ts_ms, channel: "system", text: r.line.text, is_final: true }} single /> : <GapMarker key={`g${r.gap.from_ms}`} gap={r.gap} />))}
        {interim && <TranscriptLine ev={{ ts_ms: interimAt, channel: "system", text: interim, is_final: false }} single />}
      </div>
    </section>
  );
}
