import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { formatTs, markdownToHtml } from "@web/editor/html";
import { TranscriptLine, scrollToTranscriptTime } from "@web/components/TranscriptLine";
import { storedSegmentToEvent } from "@web/lib/ws";
import { AskPill } from "@web/components/AskPill";
import { type ChatCopy, ChatWindow } from "@web/components/ChatWindow";
import type { ChatMessage } from "@web/lib/api";
import { Back, DeviceTag } from "../ui";
import type { CloudChat } from "../lib/useChat";
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
export type BodyState = { status: "loading" } | { status: "ready" } | { status: "error"; message: string; onRetry: () => void } | { status: "gone" };

const NOTE = "flex items-center justify-between gap-[8px] text-[13px] min-[721px]:text-[11.5px] rounded-button px-[12px] py-[8px] bg-blsoft text-ink";

/** `entry` is null when nothing is known of the meeting yet, as after opening a link. */
export function MeetingView({ entry, device, deviceColor, back, onBack, desktop, body, chat }: { entry: Entry | null; device: string; deviceColor: string; back: string; onBack: () => void; desktop: boolean; body: BodyState; chat: CloudChat | null }) {
  return (
    <article className="flex flex-col gap-[10px] min-[721px]:gap-[12px] flex-1 min-w-0">
      {!desktop && <Back to={back} onClick={onBack} />}
      {entry && (
        <div>
          <h1 className="text-[22px] font-extrabold tracking-[-0.01em] text-ink m-[0px]">{entry.title}</h1>
          <div className="flex flex-wrap items-center gap-[6px] min-[721px]:gap-[8px] text-[13px] min-[721px]:text-[12px] text-mut mt-[3px] min-[721px]:mt-[4px]">
            <DeviceTag name={device} color={deviceColor} />
            {started(entry)}
          </div>
        </div>
      )}
      {body.status === "loading" && (
        <p role="status" className="text-[15px] min-[721px]:text-[12px] text-mut m-[0px]">
          Loading…
        </p>
      )}
      {body.status === "gone" && (
        <p role="status" className="text-[15px] min-[721px]:text-[12px] text-mut m-[0px]">
          This meeting is no longer in your Cloud.
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
      {body.status === "ready" && entry && <MeetingBody entry={entry} device={device} onLink={() => chat?.setOpen(false)} />}
      {body.status === "ready" && chat && <Ask chat={chat} />}
    </article>
  );
}

function MeetingBody({ entry, device, onLink }: { entry: Entry; device: string; onLink: () => void }) {
  const tabs = tabsFor(entry);
  const [picked, setPicked] = useState<Tab | null>(null);
  const tab = picked && tabs.includes(picked) ? picked : defaultTab(entry);
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
          <div role="tabpanel" ref={transcript}>{entry.segments.map((s, i) => <TranscriptLine key={i} ev={storedSegmentToEvent(s)} />)}</div>
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
