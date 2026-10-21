/**
 * `stt_engine` is stamped at recording start as "local · <model>" or "cloud · <model>". Older meetings have `null` and get no engine pill.
 * A bare "local" / "cloud" (from the active-recording poll before the row refetches) renders as "Local STT" / "Cloud STT".
 * Pills show a short label (the icon already says local vs cloud, or CLI) and the full name in a hover tooltip.
 */

import { Cloud, HardDrive, Sparkles, Terminal } from "lucide-react";

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
): { cloud: boolean; text: string; full: string } | null {
  const s = raw?.trim();
  if (!s) return null;
  const [provider, ...rest] = s.split("·").map((p) => p.trim());
  const cloud = provider?.toLowerCase() === "cloud";
  const label = cloud ? "Cloud" : "Local";
  const model = rest.filter(Boolean).join(" · ");
  const text = model || `${label} STT`;
  return { cloud, text, full: model ? `${label} · ${model}` : text };
}

/** CLI stamps are `cli:<binary>[:<model>]`, and opencode models carry a `provider/` prefix; the label keeps only the last segment. */
export function parseLlmModel(
  raw: string | null | undefined,
): { cli: boolean; text: string; full: string } | null {
  const full = raw?.trim();
  if (!full) return null;
  const text = full.split(":").pop()!.split("/").pop()! || full;
  return { cli: full.startsWith("cli:"), text, full };
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
// Caps a model name we have not seen yet so it cannot squeeze the Library row's title.
const LABEL = "min-w-0 max-w-[12rem] truncate";

// A native `title` only appears after a second of holding still, so users missed it.
// Same surface as the app's menus (bg-card, border-line, shadow-pop), with the ID in mono.
function FullName({ caption, value }: { caption: string; value: string }) {
  return (
    <span
      role="tooltip"
      className="pointer-events-none absolute right-0 top-full z-20 mt-1.5 flex flex-col gap-1 whitespace-nowrap rounded-chip border border-line bg-card px-2.5 py-2 text-left leading-none shadow-pop opacity-0 translate-y-0.5 transition duration-150 ease-pop group-hover/pill:translate-y-0 group-hover/pill:opacity-100 group-hover/pill:delay-150"
    >
      <span className="text-[11px] font-medium text-mut">{caption}</span>{" "}
      <span className="font-mono text-[12px] font-normal text-ink">{value}</span>
    </span>
  );
}

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
      className={`${PILL} ${tone} relative group/pill`}
      data-testid="engine-pill"
    >
      {engine.cloud ? (
        <Cloud size={12} aria-hidden className="shrink-0" />
      ) : (
        <HardDrive size={12} aria-hidden className="shrink-0 text-matcha" />
      )}
      <span className={LABEL}>{engine.text}</span>
      {engine.full !== engine.text && (
        <FullName caption={engine.cloud ? "Cloud" : "Local"} value={engine.text} />
      )}
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
  const model = parseLlmModel(llmModel);
  if (!model) return null;
  // Filled once enhanced, outlined while pending; mirrors the STT pill's filled-vs-outlined convention.
  const tone = pending ? "border border-blue/40 text-blue bg-transparent" : "bg-blsoft text-ink";
  const Icon = model.cli ? Terminal : Sparkles;
  return (
    <span
      className={`${PILL} ${tone} relative group/pill`}
      data-testid="llm-pill"
    >
      <Icon size={12} aria-hidden className={pending ? "shrink-0" : "shrink-0 text-blue"} />
      <span className={LABEL}>{model.text}</span>
      <FullName caption={pending ? "Will enhance with" : "Enhanced by"} value={model.full} />
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
