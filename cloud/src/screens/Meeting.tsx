import { type KeyboardEvent, type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@web/components/Button";
import { EnhancingBanner } from "@web/components/EnhancingBanner";
import { formatTs, markdownToHtml } from "@web/editor/html";
import { TranscriptLine, scrollToTranscriptTime } from "@web/components/TranscriptLine";
import { storedSegmentToEvent } from "@web/lib/ws";
import { AskPill } from "@web/components/AskPill";
import { type ChatCopy, ChatWindow } from "@web/components/ChatWindow";
import type { ChatMessage } from "@web/lib/api";
import { Back, DeviceTag } from "../ui";
import type { CloudChat } from "../lib/useChat";
import { Adopted, type EditOp, type PendingEdit } from "../lib/api";
import { waitingText } from "../lib/edits";
import type { Finish } from "../lib/session";
import { defaultTab, pushedFrom, started, tabsFor, type Entry, type Tab } from "../lib/library";

/** The sanitized HTML, with each transcript link given the "↳ mm:ss" text the desktop editor draws for it. */
function render(md: string): string {
  const t = document.createElement("template");
  t.innerHTML = markdownToHtml(md);
  for (const el of t.content.querySelectorAll("[data-transcript-link]")) {
    el.textContent = `↳ ${formatTs(Number(el.getAttribute("data-ts")) || 0)}`;
    el.classList.add("transcript-link");
    el.setAttribute("role", "link");
    el.setAttribute("tabindex", "0");
  }
  return t.innerHTML;
}

const LABELS: Record<Tab, string> = { notes: "Notes", enhanced: "Enhanced", transcript: "Transcript" };

/** What the open meeting's body is doing. Until it is `ready`, `entry` carries only a header. */
export type BodyState = { status: "loading" } | { status: "ready" } | { status: "error"; message: string; onRetry: () => void } | { status: "gone"; adopted?: boolean };

/** Runs an enhance of the open meeting and pushes it, calling `onText` with the document so far. Resolves to the stored `enriched_md`, or null when the meeting is too short. */
export type Enhance = (onText: (text: string) => void) => Promise<string | null>;

type Run = { status: "idle" } | Exclude<Finish, { status: "done" }>;

/** What the open meeting can change from here. */
export interface Editing {
  /** The Mac that makes the edits, or null for a phone's own meeting, which changes at once. */
  mac: string | null;
  pending: PendingEdit[];
  /** `value` is the new title or the note's text. */
  edit: (op: EditOp, value?: string) => Promise<void>;
  cancel: (id: number) => Promise<void>;
}

const NOTE = "flex items-center justify-between gap-[8px] text-[13px] min-[721px]:text-[11.5px] rounded-button px-[12px] py-[8px] bg-blsoft text-ink";

/**
 * `entry` is null when nothing is known of the meeting yet, as after opening a link.
 * `enhance` is set for a phone meeting's live view, the only meetings the webapp writes.
 * `finish` is the enhance End started for this meeting, which shows until the user starts one here.
 * `editing` is null where nothing may change, as in History or while a phone still records the meeting.
 */
export function MeetingView({ entry, device, deviceColor, back, onBack, desktop, body, chat, enhance, finish, editing }: { entry: Entry | null; device: string; deviceColor: string; back: string; onBack: () => void; desktop: boolean; body: BodyState; chat: CloudChat | null; enhance?: Enhance | null; finish?: Finish | null; editing?: Editing | null }) {
  const [own, setRun] = useState<Run>({ status: "idle" });
  const [took, setTook] = useState(false);
  const outside = took ? null : finish;
  const run: Run = outside && outside.status !== "done" ? outside : own;
  // The stored result shows until the reload after the push brings it.
  const [stored, setEnhanced] = useState<string | null>(null);
  const enhanced = stored ?? (outside?.status === "done" ? outside.enriched_md : null);
  const start = async () => {
    if (!enhance || own.status === "running") return;
    setTook(true);
    setRun({ status: "running", text: "" });
    try {
      const md = await enhance((text) => setRun({ status: "running", text }));
      if (md === null) return setRun({ status: "short" });
      setEnhanced(md);
      setRun({ status: "idle" });
    } catch (e) {
      // The body says the meeting is the Mac's now.
      if (e instanceof Adopted) return setRun({ status: "idle" });
      setRun({ status: "error", message: `Enhance failed. ${e instanceof Error ? e.message : String(e)}` });
    }
  };
  const ready = body.status === "ready" && entry;
  return (
    <article className="flex flex-col gap-[10px] min-[721px]:gap-[12px] flex-1 min-w-0">
      {(run.status === "running" || run.status === "error") && (
        // Full width at the top of the screen, past the column's padding.
        <div className={`sticky top-0 z-20 ${desktop ? "-mx-[28px] -mt-[22px]" : "-mx-[16px] -mt-[16px]"}`}>
          <EnhancingBanner
            visible
            stacked={!desktop}
            chars={run.status === "running" ? run.text.length : undefined}
            errorMessage={run.status === "error" ? run.message : null}
            onRetry={run.status === "error" && run.retry ? run.retry : () => void start()}
          />
        </div>
      )}
      {!desktop && <Back to={back} onClick={onBack} />}
      {entry && (
        <div>
          <div className="flex items-start gap-[10px]">
            <h1 className="flex-1 min-w-0 text-[22px] font-extrabold tracking-[-0.01em] text-ink m-[0px]">{entry.title}</h1>
            {ready && enhance && run.status === "idle" && (
              <Button size="sm" onClick={() => void start()} className="flex-none">
                {(enhanced ?? entry.enhanced)?.trim() ? "Re-enhance" : "Enhance"}
              </Button>
            )}
          </div>
          <div className="flex flex-wrap items-center gap-[6px] min-[721px]:gap-[8px] text-[13px] min-[721px]:text-[12px] text-mut mt-[3px] min-[721px]:mt-[4px]">
            <DeviceTag name={device} color={deviceColor} />
            {started(entry)}
          </div>
          {ready && editing && <EditBar key={entry.key} editing={editing} title={entry.title} />}
        </div>
      )}
      {body.status === "loading" && (
        <p role="status" className="text-[15px] min-[721px]:text-[12px] text-mut m-[0px]">
          Loading…
        </p>
      )}
      {body.status === "gone" && (
        <p role="status" className="text-[15px] min-[721px]:text-[12px] text-mut m-[0px]">
          {body.adopted ? "A Mac took this meeting, so it now lists under that Mac." : "This meeting is no longer in your Cloud."}
        </p>
      )}
      {body.status === "error" && (
        <div role="alert" className={NOTE}>
          <span className="min-w-0 break-words">Could not load this meeting. {body.message}</span>
          <button type="button" onClick={body.onRetry} className="flex-none text-blue font-semibold cursor-pointer bg-transparent border-0 p-[0px] font-[inherit]">
            Retry
          </button>
        </div>
      )}
      {run.status === "short" && (
        <p role="status" className="text-[15px] min-[721px]:text-[12px] text-mut m-[0px]">
          This meeting is too short to enhance.
        </p>
      )}
      {ready && (
        <MeetingBody
          entry={{ ...entry, enhanced: run.status === "running" ? run.text : (enhanced ?? entry.enhanced) }}
          streaming={run.status === "running"}
          device={device}
          onLink={() => chat?.setOpen(false)}
        />
      )}
      {body.status === "ready" && chat && <Ask chat={chat} />}
    </article>
  );
}

const ACTION = "text-[14px] min-[721px]:text-[12.5px] font-semibold bg-transparent border-0 p-[0px] cursor-pointer font-[inherit] disabled:opacity-50 disabled:cursor-not-allowed";
const INPUT = "w-full font-sans text-[16px] min-[721px]:text-[13px] bg-paper text-ink placeholder:text-grey border border-line rounded-button px-[12px] py-[10px] outline-none focus:border-blue focus:ring-1 focus:ring-blue";

type Mode = "rename" | "note" | "delete";

/** Rename, Add note, and Delete, each opening its form under the header, then a line for each edit the Mac has yet to make. */
function EditBar({ editing, title }: { editing: Editing; title: string }) {
  const [mode, setMode] = useState<Mode | null>(null);
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const open = (m: Mode) => {
    setMode(m);
    setText(m === "rename" ? title : "");
    setError(null);
  };
  const run = async (f: () => Promise<void>) => {
    setBusy(true);
    setError(null);
    try {
      await f();
      setMode(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  // Undo leaves an open form alone.
  const undo = async (id: number) => {
    setError(null);
    try {
      await editing.cancel(id);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };
  const save = () => void run(() => (mode === "rename" ? editing.edit("rename", text) : mode === "note" ? editing.edit("append_note", text) : editing.edit("delete")));
  const ready = mode === "delete" || (mode === "rename" ? !!text.trim() && text.trim() !== title : !!text.trim());
  const keys = (e: KeyboardEvent) => {
    if (e.key === "Escape") setMode(null);
    else if (e.key === "Enter" && (mode === "rename" || e.metaKey || e.ctrlKey) && ready && !busy) {
      e.preventDefault();
      save();
    }
  };
  return (
    <div className="flex flex-col gap-[8px] mt-[10px] min-[721px]:max-w-[560px]">
      {!mode && (
        <div className="flex gap-[16px]">
          <button type="button" className={`${ACTION} text-blue`} onClick={() => open("rename")}>
            Rename
          </button>
          <button type="button" className={`${ACTION} text-blue`} onClick={() => open("note")}>
            Add note
          </button>
          <button type="button" className={`${ACTION} text-straw`} onClick={() => open("delete")}>
            Delete
          </button>
        </div>
      )}
      {mode && (
        <form
          className="flex flex-col gap-[8px]"
          onSubmit={(e) => {
            e.preventDefault();
            if (ready && !busy) save();
          }}
        >
          {mode === "rename" && <input aria-label="New title" autoFocus value={text} onChange={(e) => setText(e.target.value)} onKeyDown={keys} maxLength={500} className={INPUT} />}
          {mode === "note" && (
            <textarea aria-label="New note" autoFocus rows={4} value={text} onChange={(e) => setText(e.target.value)} onKeyDown={keys} maxLength={20000} placeholder="Added to the end of your notes" className={`${INPUT} resize-y`} />
          )}
          {mode === "delete" && (
            <p className="text-[15px] min-[721px]:text-[12.5px] text-ink m-[0px]">
              {editing.mac ? `Delete this meeting? ${editing.mac} deletes it, and your Cloud keeps it in History.` : "Delete this meeting? Your Cloud keeps it in History."}
            </p>
          )}
          <div className="flex gap-[8px]">
            <Button size="sm" type="submit" variant={mode === "delete" ? "warn" : "primary"} disabled={!ready || busy}>
              {mode === "rename" ? "Save title" : mode === "note" ? "Add note" : "Delete"}
            </Button>
            <Button size="sm" variant="secondary" onClick={() => setMode(null)} disabled={busy}>
              Cancel
            </Button>
          </div>
        </form>
      )}
      {error && (
        <div role="alert" className={NOTE}>
          <span className="min-w-0 break-words">{error}</span>
        </div>
      )}
      {editing.mac &&
        editing.pending.map((e) => (
          <Waiting key={e.id} text={waitingText(e, editing.mac!)} onUndo={() => void undo(e.id)} />
        ))}
    </div>
  );
}

function Waiting({ text, onUndo }: { text: string; onUndo: () => void }) {
  return (
    <div role="status" className={NOTE}>
      <span className="min-w-0 break-words">{text}</span>
      <button type="button" onClick={onUndo} className={`${ACTION} flex-none text-blue`}>
        Undo
      </button>
    </div>
  );
}

/** While `streaming`, Enhanced shows the document so far and opens unless another tab is picked. */
function MeetingBody({ entry, streaming, device, onLink }: { entry: Entry; streaming: boolean; device: string; onLink: () => void }) {
  const has = tabsFor(entry);
  const tabs = streaming ? (Object.keys(LABELS) as Tab[]).filter((t) => t === "enhanced" || has.includes(t)) : has;
  const [picked, setPicked] = useState<Tab | null>(null);
  // Settles on Enhanced once the stream ends, as the default tab of an enhanced meeting.
  useEffect(() => {
    if (streaming) setPicked(null);
  }, [streaming]);
  const tab = picked && tabs.includes(picked) ? picked : streaming ? "enhanced" : defaultTab(entry);
  // A link tap records its time here and the tab switch renders the lines; the effect then scrolls once and clears it.
  const pendingSec = useRef<number | null>(null);
  const transcript = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (tab !== "transcript" || pendingSec.current === null) return;
    if (transcript.current) scrollToTranscriptTime(transcript.current, pendingSec.current);
    pendingSec.current = null;
  }, [tab]);
  return (
    <>
      {tabs.length > 1 && (
        <div role="tablist" className="flex gap-[4px] border-b border-line">
          {tabs.map((t) => (
            <button
              key={t}
              type="button"
              role="tab"
              aria-selected={t === tab}
              onClick={() => setPicked(t)}
              className={`text-[14px] min-[721px]:text-[12px] font-bold px-[10px] py-[8px] min-[721px]:py-[7px] bg-transparent border-0 cursor-pointer font-[inherit] ${t === tab ? "text-blue shadow-[inset_0_-2px_0_var(--color-blue)]" : "text-mut"}`}
            >
              {LABELS[t]}
            </button>
          ))}
        </div>
      )}
      <div className="cloud-doc text-[16px] min-[721px]:text-[13px] leading-[1.55]">
        {tab === "transcript" ? (
          <div role="tabpanel" ref={transcript}>{entry.segments.map((s, i) => <TranscriptLine key={i} ev={storedSegmentToEvent(s)} single={entry.phone} />)}</div>
        ) : (
          <Doc md={tab === "enhanced" ? entry.enhanced ?? "" : entry.notes} onLink={tabs.includes("transcript") ? (sec) => { pendingSec.current = sec; setPicked("transcript"); onLink(); } : undefined} />
        )}
      </div>
      <p className="text-[13px] min-[721px]:text-[11.5px] text-mut mt-auto pt-[10px] mb-[0px]">{pushedFrom(device, entry.pushedAt)}</p>
    </>
  );
}

function Doc({ md, onLink }: { md: string; onLink?: (sec: number) => void }) {
  const html = useMemo(() => render(md), [md]);
  const open = (e: { target: EventTarget }) => {
    const link = e.target instanceof Element ? e.target.closest("[data-transcript-link]") : null;
    if (link) onLink?.(Number(link.getAttribute("data-ts")) || 0);
  };
  // The markup is sanitized by markdownToHtml, the same allowlist the desktop editor loads.
  return <div role="tabpanel" className="doc" onClick={open} onKeyDown={(e) => {
        if (e.key !== "Enter" && e.key !== " ") return;
        if (e.target instanceof Element && e.target.closest("[data-transcript-link]")) e.preventDefault();
        open(e);
      }} dangerouslySetInnerHTML={{ __html: html }} />;
}

// Sticks to the bottom of the screen while the column scrolls under it.
export const DOCKED = "cloud-chat sticky bottom-[16px] self-center w-full max-w-[480px]";

export function Ask({ chat, copy, renderExtra, position = DOCKED }: { chat: CloudChat; copy?: ChatCopy; renderExtra?: (m: ChatMessage) => ReactNode; position?: string }) {
  return chat.open ? (
    <ChatWindow messages={chat.messages} streamingId={chat.streamingId} onSend={chat.send} onCollapse={() => chat.setOpen(false)} position={position} copy={copy} renderExtra={renderExtra} />
  ) : (
    <AskPill onExpand={() => chat.setOpen(true)} position={position} label={copy?.placeholder} />
  );
}
