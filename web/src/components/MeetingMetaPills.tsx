/**
 * `stt_engine` is stamped at recording start as "local · <model>" or "cloud · <model>". Older meetings have `null` and get no engine pill.
 * A bare "local" / "cloud" (from the active-recording poll before the row refetches) renders as "Local STT" / "Cloud STT".
 * Pills show a short label (the icon already says local vs cloud, or CLI) and the full name in a hover tooltip.
 */

import { useEffect, useRef, useState } from "react";
import { Link } from "react-router";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Check, ChevronDown, Cloud, HardDrive, Sparkles, Terminal } from "lucide-react";
import {
  settingsApi,
  settingsKey,
  useSettings,
  type ProviderView,
  type SettingsView,
} from "../lib/api/settings";
import { LABEL as CAPTION } from "./settings/labelClass";

interface Props {
  startedAt?: number | null;
  endedAt?: number | null;
  sttEngine?: string | null;
  llmModel?: string | null;
  /** True while the model has not enhanced this meeting yet (live header), so the pill becomes the provider switch. */
  llmPending?: boolean;
  /** Set while Enhance runs: it already resolved its provider, so a switch would only mislabel the pill. */
  llmLocked?: boolean;
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

/** The id a provider stamps as `llm_model` once it enhances, so the live pill reads the same as the post page will. */
export function providerModelId(p: ProviderView): string {
  if (p.adapter !== "cli") return p.model;
  return p.cli_model ? `cli:${p.model}:${p.cli_model}` : `cli:${p.model}`;
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

/** `llm_model` is stamped by enhance.rs after a successful enhance. */
export function LlmPill({ llmModel }: { llmModel: string | null | undefined }) {
  const model = parseLlmModel(llmModel);
  if (!model) return null;
  const Icon = model.cli ? Terminal : Sparkles;
  return (
    <span
      className={`${PILL} bg-blsoft text-ink relative group/pill`}
      data-testid="llm-pill"
    >
      <Icon size={12} aria-hidden className="shrink-0 text-blue" />
      <span className={LABEL}>{model.text}</span>
      <FullName caption="Enhanced by" value={model.full} />
    </span>
  );
}

/** The live header's pill: picking a provider activates it globally, and the next recap tick and the Enhance at End read it. */
export function LlmSwitch({ llmModel, disabled = false }: { llmModel: string; disabled?: boolean }) {
  const model = parseLlmModel(llmModel);
  const [open, setOpen] = useState(false);
  const wrapperRef = useRef<HTMLSpanElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const settings = useSettings().data;
  const providers = settings?.providers ?? [];
  const qc = useQueryClient();
  const activate = useMutation({
    mutationFn: (id: string) => settingsApi.activateProvider(id),
    onSuccess: (_view, id) => {
      // Flip the cached flag so the pill changes with the menu closing, not a refetch later.
      qc.setQueryData<SettingsView>(settingsKey, (s) =>
        s && { ...s, providers: s.providers.map((p) => ({ ...p, is_active: p.id === id })) },
      );
      close();
      return qc.invalidateQueries({ queryKey: settingsKey });
    },
  });
  const setModel = useMutation({
    mutationFn: ({ p, cli_model }: { p: ProviderView; cli_model: string }) =>
      settingsApi.updateProvider(p.id, { name: p.name, base_url: p.base_url, model: p.model, cli_model }),
    onSuccess: (view) => {
      qc.setQueryData<SettingsView>(settingsKey, (s) =>
        s && { ...s, providers: s.providers.map((p) => (p.id === view.id ? view : p)) },
      );
      close();
      return qc.invalidateQueries({ queryKey: settingsKey });
    },
  });
  const busy = activate.isPending || setModel.isPending;

  // The picked item was disabled while saving, which dropped focus, so hand it back to the pill.
  function close() {
    setOpen(false);
    wrapperRef.current?.querySelector("button")?.focus();
  }
  const failed = activate.error ?? setModel.error;

  useEffect(() => {
    if (!open) return;
    const menu = menuRef.current;
    const first =
      menu?.querySelector<HTMLElement>('[aria-checked="true"]') ??
      menu?.querySelector<HTMLElement>('[role^="menuitem"]');
    first?.focus();
    const onDown = (e: MouseEvent) => {
      if (!wrapperRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  function onMenuKey(e: React.KeyboardEvent) {
    if (e.key === "Escape") {
      // Stopped here so the chat window's own Escape shortcut does not also fire.
      e.stopPropagation();
      close();
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    e.preventDefault();
    const items = [...(menuRef.current?.querySelectorAll<HTMLElement>('[role^="menuitem"]') ?? [])];
    const i = items.indexOf(document.activeElement as HTMLElement);
    const next = e.key === "ArrowDown" ? i + 1 : i - 1;
    items[(next + items.length) % items.length]?.focus();
  }

  if (!model) return null;
  const Icon = model.cli ? Terminal : Sparkles;
  const tone = open
    ? "border border-blue text-blue bg-blsoft"
    : "border border-blue/40 text-blue bg-transparent enabled:hover:bg-blsoft/50";
  return (
    <span ref={wrapperRef} className="relative" onKeyDown={open ? onMenuKey : undefined}>
      <button
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => {
          if (!busy) {
            activate.reset();
            setModel.reset();
          }
          setOpen((o) => !o);
        }}
        className={`${PILL} ${tone} relative group/pill cursor-pointer transition-colors disabled:cursor-default focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-blue`}
        data-testid="llm-pill"
      >
        <Icon size={12} aria-hidden className="shrink-0" />
        <span className={LABEL}>{model.text}</span>
        {!disabled && (
          <ChevronDown
            size={12}
            aria-hidden
            className={`shrink-0 -mr-0.5 transition-transform ${open ? "rotate-180" : ""}`}
          />
        )}
        {!open && <FullName caption="Will enhance with" value={model.full} />}
      </button>
      {open && (
        <div className="absolute left-0 top-full mt-1.5 bg-card border border-line rounded-card shadow-pop py-1 min-w-[280px] max-w-[min(360px,calc(100vw-32px))] z-20 text-[13px]">
          <p aria-hidden className={`${CAPTION} px-3 pt-1.5 pb-1`}>
            Recap and enhance with
          </p>
          <div ref={menuRef} role="menu" aria-label="Recap and enhance with">
            {providers.map((p) => {
              const RowIcon = p.adapter === "cli" ? Terminal : Sparkles;
              // Only a CLI preset's curated aliases always resolve: an http or opencode list can name a model the user's
              // setup does not serve, failing the next recap. An empty cli_model means Settings' Test has not passed yet.
              const preset =
                p.is_active && p.adapter === "cli" && p.cli_model
                  ? settings?.presets.find((x) => x.adapter === "cli" && x.default_model === p.model)
                  : undefined;
              const models = preset && preset.models.length > 1 ? [...new Set([...preset.models, p.cli_model])] : [];
              const chips = models.length > 1;
              return (
                <div key={p.id}>
                  <button
                    type="button"
                    role="menuitemradio"
                    aria-checked={p.is_active}
                    disabled={busy}
                    onClick={() => (p.is_active ? setOpen(false) : activate.mutate(p.id))}
                    className="w-full flex items-center gap-2.5 px-3 py-1.5 text-left hover:bg-paper focus-visible:bg-paper focus-visible:outline-none disabled:cursor-wait"
                  >
                    <RowIcon
                      size={14}
                      aria-hidden
                      className={`shrink-0 ${p.is_active ? "text-blue" : "text-mut"}`}
                    />
                    <span className="flex-1 min-w-0 flex flex-col gap-0.5">
                      <span className="truncate text-ink font-semibold">{p.name}</span>
                      {!chips && (
                        <span className="truncate font-mono text-[11.5px] text-mut">
                          {providerModelId(p)}
                        </span>
                      )}
                    </span>
                    {p.is_active ? (
                      <Check size={14} aria-hidden className="text-blue shrink-0" />
                    ) : (
                      <span className="w-[14px] shrink-0" aria-hidden />
                    )}
                  </button>
                  {chips && (
                    <div
                      role="group"
                      aria-label={`${p.name} model`}
                      className="flex flex-wrap gap-1 pl-[36px] pr-3 pb-2 pt-0.5"
                    >
                      {models.map((m) => {
                        const on = m === p.cli_model;
                        return (
                          <button
                            key={m}
                            type="button"
                            role="menuitemradio"
                            aria-checked={on}
                            disabled={busy}
                            onClick={() => (on ? setOpen(false) : setModel.mutate({ p, cli_model: m }))}
                            className={`rounded-pill px-2 py-[3px] font-mono text-[11.5px] leading-none border transition-colors focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-blue disabled:cursor-wait ${
                              on ? "bg-blsoft border-blue/50 text-blue" : "border-line text-ink hover:bg-paper"
                            }`}
                          >
                            {m}
                          </button>
                        );
                      })}
                    </div>
                  )}
                </div>
              );
            })}
            <div role="separator" className="my-1 border-t border-line" />
            <Link
              to="/settings/model"
              role="menuitem"
              className="block px-3 py-1.5 text-blue hover:bg-paper focus-visible:bg-paper focus-visible:outline-none"
            >
              Model settings
            </Link>
          </div>
          {failed && (
            <p role="alert" className="px-3 py-1.5 text-[12px] leading-snug text-straw">
              {failed.message}
            </p>
          )}
          <p className="px-3 pt-0.5 pb-1.5 text-[12px] leading-snug text-mut">
            Also the default for new meetings.
          </p>
        </div>
      )}
    </span>
  );
}

export function MeetingMetaPills({
  startedAt,
  endedAt,
  sttEngine,
  llmModel,
  llmPending,
  llmLocked,
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
      {model && (llmPending ? <LlmSwitch llmModel={model} disabled={llmLocked} /> : <LlmPill llmModel={model} />)}
    </div>
  );
}
