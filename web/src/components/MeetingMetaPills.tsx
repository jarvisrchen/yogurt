/**
 * `stt_engine` is stamped at recording start as "local · <model>" or "cloud · <model>". Older meetings have `null` and get no engine pill.
 * A bare "local" / "cloud" (from the active-recording poll before the row refetches) renders as "Local STT" / "Cloud STT".
 */

import { Cloud, HardDrive, Sparkles } from "lucide-react";

interface Props {
  startedAt?: number | null;
  endedAt?: number | null;
  sttEngine?: string | null;
  llmModel?: string | null;
  /** True while the model has not enhanced this meeting yet (live header),
   *  so the pill reads as "will enhance with" rather than "enhanced by". */
  llmPending?: boolean;
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
  "inline-flex items-center gap-1.5 rounded-pill px-2.5 py-1 text-[12px] font-medium leading-none";

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
        tone === "warn" ? "bg-strsoft text-ink" : "bg-card text-mut border border-line"
      }`}
    >
      {children}
    </span>
  );
}

export function EnginePill({ sttEngine }: { sttEngine: string | null | undefined }) {
  const engine = parseSttEngine(sttEngine);
  if (!engine) return null;
  // Filled matcha = on this Mac, neutral card = went to a provider.
  const tone = engine.cloud ? "bg-card text-mut border border-line" : "bg-mtsoft text-ink";
  return (
    <span
      className={`${PILL} ${tone}`}
      data-testid="engine-pill"
      title={engine.cloud ? "Transcribed by cloud STT" : "Transcribed on this Mac"}
    >
      {engine.cloud ? (
        <Cloud size={12} aria-hidden />
      ) : (
        <HardDrive size={12} aria-hidden className="text-matcha" />
      )}
      {engine.text}
    </span>
  );
}

/** `llm_model` is stamped by enhance.rs after a successful enhance. The live header passes the active model with `pending`. */
export function LlmPill({
  llmModel,
  pending = false,
}: {
  llmModel: string | null | undefined;
  pending?: boolean;
}) {
  const model = llmModel?.trim();
  if (!model) return null;
  // Filled once enhanced, outlined while pending; mirrors the STT pill's filled-vs-outlined convention.
  const tone = pending ? "border border-blue/40 text-blue bg-transparent" : "bg-blsoft text-ink";
  return (
    <span
      className={`${PILL} ${tone}`}
      data-testid="llm-pill"
      title={pending ? `Will enhance with ${model}` : `Enhanced by ${model}`}
    >
      <Sparkles size={12} aria-hidden className={pending ? undefined : "text-blue"} />
      {model}
    </span>
  );
}

export function MeetingMetaPills({
  startedAt,
  endedAt,
  sttEngine,
  llmModel,
  llmPending,
}: Props) {
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
      {model && <LlmPill llmModel={model} pending={llmPending} />}
    </div>
  );
}
