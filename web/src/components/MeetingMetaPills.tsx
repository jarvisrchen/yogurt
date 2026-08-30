/**
 * `MeetingMetaPills` - the meeting's metadata as a row of small pills:
 *
 *   [Aug 28 · 1:08 PM] [8 min] [Local · small.en]
 *
 * Shared by the live meeting header and the post-meeting header so both
 * views read the same facts the same way. Everything comes from the
 * persisted meeting row (`started_at`, `ended_at`, `stt_engine`), so the
 * engine pill keeps showing after recording stops - it is meeting
 * metadata, not a live status indicator.
 *
 * `stt_engine` is stamped at recording start as "local · <model>" or
 * "cloud · <model>" (routes.rs `start_meeting`). Meetings recorded before
 * that column existed have `null` and simply get no engine pill. A bare
 * provider ("local" / "cloud", from the active-recording poll before the
 * row refetches) renders as "Local STT" / "Cloud STT".
 */

import { Cloud, HardDrive, Sparkles } from "lucide-react";

interface Props {
  startedAt?: number | null;
  endedAt?: number | null;
  sttEngine?: string | null;
  llmModel?: string | null;
}

export function parseSttEngine(
  raw: string | null | undefined,
): { cloud: boolean; text: string } | null {
  const s = raw?.trim();
  if (!s) return null;
  const [provider, ...rest] = s.split("·").map((p) => p.trim());
  const cloud = provider?.toLowerCase() === "cloud";
  const label = cloud ? "Cloud" : "Local";
  const model = rest.filter(Boolean).join(" · ");
  return { cloud, text: model ? `${label} · ${model}` : `${label} STT` };
}

export function formatStartedAt(startedAt: number): string {
  const d = new Date(startedAt);
  return `${d.toLocaleDateString(undefined, { month: "short", day: "numeric" })} · ${d.toLocaleTimeString(
    undefined,
    { hour: "numeric", minute: "2-digit" },
  )}`;
}

export function formatDuration(startedAt: number, endedAt: number): string | null {
  if (endedAt <= startedAt) return null;
  return `${Math.max(1, Math.round((endedAt - startedAt) / 60_000))} min`;
}

const PILL =
  "inline-flex items-center gap-1 rounded-pill px-2 py-0.5 text-[11px] font-mono leading-none";

/** Neutral metadata pill (date, time, duration). `tone="warn"` flags an
 *  exception such as "not enhanced" in strawberry. */
export function MetaPill({
  children,
  tone = "neutral",
}: {
  children: React.ReactNode;
  tone?: "neutral" | "warn";
}) {
  return (
    <span
      className={`${PILL} ${
        tone === "warn" ? "bg-strsoft text-straw" : "border border-line bg-paper text-mut"
      }`}
    >
      {children}
    </span>
  );
}

/**
 * The STT engine pill on its own - shared with the Library card so the
 * same meeting reads identically everywhere. Renders nothing for `null`
 * (meetings recorded before the column existed) rather than guessing.
 */
export function EnginePill({ sttEngine }: { sttEngine: string | null | undefined }) {
  const engine = parseSttEngine(sttEngine);
  if (!engine) return null;
  // Option E: STT family is always matcha. Filled = on this Mac,
  // outlined (#CBE0D2 border per the lavish mockup) = went to a provider.
  const tone = engine.cloud
    ? "border border-[#CBE0D2] bg-transparent text-matcha"
    : "bg-mtsoft text-matcha";
  return (
    <span
      className={`${PILL} ${tone}`}
      data-testid="engine-pill"
      title={engine.cloud ? "Transcribed by cloud STT" : "Transcribed on this Mac"}
    >
      {engine.cloud ? <Cloud size={11} aria-hidden /> : <HardDrive size={11} aria-hidden />}
      {engine.text}
    </span>
  );
}

/**
 * The LLM pill - which model fused the notes with the transcript. The
 * row's `llm_model` is stamped by enhance.rs after a successful enhance
 * (bare model name, e.g. "gpt-5-mini"), so a recorded-but-never-enhanced
 * meeting renders nothing rather than a guessed model. The LLM is always
 * a BYO OpenAI-compatible endpoint, so there is no local/cloud split.
 */
export function LlmPill({ llmModel }: { llmModel: string | null | undefined }) {
  const model = llmModel?.trim();
  if (!model) return null;
  // Option E: LLM family is always blueberry. Outlined (#C5BEEF border per
  // the lavish mockup) reads as "AI touched this" without competing with
  // the brand-blue button chrome.
  return (
    <span
      className={`${PILL} border border-[#C5BEEF] bg-transparent text-blue`}
      data-testid="llm-pill"
      title={`Enhanced by ${model}`}
    >
      <Sparkles size={11} aria-hidden />
      {model}
    </span>
  );
}

export function MeetingMetaPills({ startedAt, endedAt, sttEngine, llmModel }: Props) {
  const engine = parseSttEngine(sttEngine);
  const model = llmModel?.trim();
  const duration =
    startedAt != null && endedAt != null ? formatDuration(startedAt, endedAt) : null;
  if (startedAt == null && !engine && !model) return null;
  return (
    <div className="flex flex-wrap items-center gap-1.5" data-testid="meeting-meta">
      {startedAt != null && <MetaPill>{formatStartedAt(startedAt)}</MetaPill>}
      {duration && <MetaPill>{duration}</MetaPill>}
      {engine && <EnginePill sttEngine={sttEngine} />}
      {model && <LlmPill llmModel={model} />}
    </div>
  );
}
