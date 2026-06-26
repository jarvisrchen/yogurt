/**
 * STTPicker — Transcription section card pair (Phase 8 Plan 08-03).
 *
 * Wires the previously visual-only Phase-5 stub to the real STT settings
 * surface. Renders a 2-column grid with `CloudSTTCard` (Phase 5 Deepgram
 * pill row) on the left and `LocalSTTCard` (whisper.cpp picker + download
 * dialog) on the right. Clicking "Use Cloud" / "Use Local" PATCHes
 * `stt_provider`; clicking a downloaded local-model pill PATCHes
 * `stt_model`.
 *
 * Backward-compat: `<STTPicker />` is still mounted by
 * `web/src/routes/Settings.tsx`; the surface is now data-driven instead
 * of static.
 */
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { settingsApi, type General } from "../../lib/api/settings";
import { LocalSTTCard } from "./LocalSTTCard";

export function STTPicker() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["settings"], queryFn: settingsApi.get });
  const patch = useMutation({
    mutationFn: (p: Partial<General>) => settingsApi.patch(p),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });

  if (q.isLoading || !q.data) {
    return (
      <p className="text-[11px] font-mono text-mut">Loading transcription…</p>
    );
  }
  const general = q.data.general;
  const isLocal = general.stt_provider === "local";
  const selectedModel = general.stt_model || "small.en";

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      {/* ─── Cloud card ─────────────────────────────────────────────── */}
      <article
        data-testid="cloud-stt-card"
        className={
          "rounded-xl p-5 space-y-3 bg-white transition-colors " +
          (!isLocal
            ? "border-[1.5px] border-[var(--color-blue)]"
            : "border border-neutral-300")
        }
      >
        <header className="flex items-center justify-between">
          <h3 className="font-serif text-xl">Cloud</h3>
          <label className="inline-flex items-center gap-2 text-[11px] font-mono uppercase tracking-wider cursor-pointer">
            <input
              type="radio"
              name="stt-provider"
              checked={!isLocal}
              onChange={() => patch.mutate({ stt_provider: "cloud" })}
              className="accent-[var(--color-blue)]"
            />
            <span>Use Cloud</span>
          </label>
        </header>
        <p className="text-[13px] text-mut">
          Real-time partials, ~2s end-to-end. Audio is sent to the
          provider.
        </p>
        <div className="flex flex-wrap gap-2 pt-1">
          <span className="text-[11px] font-mono uppercase tracking-wider px-2.5 py-1 rounded-full bg-[var(--color-blsoft)] text-[var(--color-blue)]">
            Deepgram
          </span>
          <span className="text-[11px] font-mono uppercase tracking-wider px-2.5 py-1 rounded-full border border-neutral-300 text-neutral-500">
            AssemblyAI
          </span>
          <span className="text-[11px] font-mono uppercase tracking-wider px-2.5 py-1 rounded-full border border-neutral-300 text-neutral-500">
            Groq
          </span>
        </div>
      </article>

      {/* ─── Local card (Phase 8 — was "Coming in v1") ──────────────── */}
      <LocalSTTCard
        active={isLocal}
        selectedModel={selectedModel}
        onSelectModel={(name) =>
          patch.mutate({ stt_provider: "local", stt_model: name })
        }
        onActivate={() =>
          patch.mutate({
            stt_provider: "local",
            stt_model: selectedModel,
          })
        }
      />
    </div>
  );
}
