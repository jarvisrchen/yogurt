import { useEffect, useMemo, useRef, useState } from "react";
import { formatTs, markdownToHtml } from "@web/editor/html";
import { TranscriptLine, scrollToTranscriptTime } from "@web/components/TranscriptLine";
import { storedSegmentToEvent } from "@web/lib/ws";
import { Back, DeviceTag } from "../ui";
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

export function MeetingView({ entry, device, deviceIndex, back, onBack, desktop }: { entry: Entry; device: string; deviceIndex: number; back: string; onBack: () => void; desktop: boolean }) {
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
    <article className="flex flex-col gap-[10px] min-[721px]:gap-[12px] flex-1 min-w-0">
      {!desktop && <Back to={back} onClick={onBack} />}
      <div>
        <h1 className="text-[16px] min-[721px]:text-[22px] font-extrabold tracking-[-0.01em] text-ink m-[0px]">{entry.title}</h1>
        <div className="flex flex-wrap items-center gap-[6px] min-[721px]:gap-[8px] text-[10.5px] min-[721px]:text-[12px] text-mut mt-[3px] min-[721px]:mt-[4px]">
          <DeviceTag name={device} index={deviceIndex} />
          {started(entry)}
        </div>
      </div>
      {tabs.length > 1 && (
        <div role="tablist" className="flex gap-[4px] border-b border-line">
          {tabs.map((t) => (
            <button
              key={t}
              type="button"
              role="tab"
              aria-selected={t === tab}
              onClick={() => setPicked(t)}
              className={`text-[11px] min-[721px]:text-[12px] font-bold px-[8px] py-[6px] min-[721px]:px-[10px] min-[721px]:py-[7px] bg-transparent border-0 cursor-pointer font-[inherit] ${t === tab ? "text-blue shadow-[inset_0_-2px_0_var(--color-blue)]" : "text-mut"}`}
            >
              {LABELS[t]}
            </button>
          ))}
        </div>
      )}
      <div className="cloud-doc text-[11.5px] min-[721px]:text-[13px] leading-[1.55]">
        {tab === "transcript" ? (
          <div role="tabpanel" ref={transcript}>{entry.segments.map((s, i) => <TranscriptLine key={i} ev={storedSegmentToEvent(s)} />)}</div>
        ) : (
          <Doc md={tab === "enhanced" ? entry.enhanced ?? "" : entry.notes} onLink={tabs.includes("transcript") ? (sec) => { pendingSec.current = sec; setPicked("transcript"); } : undefined} />
        )}
      </div>
      <p className="text-[11.5px] text-mut mt-auto pt-[10px] mb-[0px]">{pushedFrom(device, entry.pushedAt)}</p>
    </article>
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
