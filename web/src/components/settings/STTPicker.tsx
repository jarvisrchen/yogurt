/**
 * STTPicker — Transcription section card pair (Phase 8 Plan 08-03).
 *
 * Wires the previously visual-only Phase-5 stub to the real STT settings
 * surface. Renders a 2-column grid with `CloudSTTCard` (Deepgram-only —
 * see below) on the left and `LocalSTTCard` (whisper.cpp picker + download
 * dialog) on the right. Clicking "Use Cloud" / "Use Local" PATCHes
 * `stt_provider`; clicking a downloaded local-model pill PATCHes
 * `stt_model`.
 *
 * Backward-compat: `<STTPicker />` is still mounted by
 * `web/src/routes/Settings.tsx`; the surface is now data-driven instead
 * of static.
 *
 * Fast task (deepgram key UX) — the Cloud card only ever supported
 * Deepgram server-side, so the AssemblyAI/Groq pills that used to sit
 * next to it were pure decoration for providers that don't exist. Removed
 * them and replaced the pill row with the same masked-key UX
 * `<ProviderCard>` uses for LLM providers: `deepgram_key_masked` from
 * `GET /api/settings`, a paste-key input, and `POST /api/settings/stt/key`.
 */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { settingsApi, type General } from "../../lib/api/settings";
import { LocalSTTCard } from "./LocalSTTCard";

/** `http()` throws `Error("<status> <statusText>: <raw body>")`. The server's
 *  422 body is `{"error": "<msg>"}` (see settings.rs's `Error::Unprocessable`)
 *  — pull just the message out so the UI shows the actual sentence instead
 *  of a status-code-prefixed JSON blob. Falls back to the raw message for
 *  anything that isn't that shape. */
function patchErrorMessage(err: unknown): string {
  const raw = err instanceof Error ? err.message : String(err);
  const jsonStart = raw.indexOf("{");
  if (jsonStart === -1) return raw;
  try {
    const parsed = JSON.parse(raw.slice(jsonStart)) as { error?: string };
    return parsed.error ?? raw;
  } catch {
    return raw;
  }
}

export function STTPicker() {
  const qc = useQueryClient();
  const q = useQuery({ queryKey: ["settings"], queryFn: settingsApi.get });
  const patch = useMutation({
    mutationFn: (p: Partial<General>) => settingsApi.patch(p),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  const [keyDraft, setKeyDraft] = useState("");
  const setSttKey = useMutation({
    mutationFn: (k: string) => settingsApi.setSttKey(k),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["settings"] });
      setKeyDraft("");
    },
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
    <div className="space-y-3">
      {patch.isError && (
        <p
          data-testid="stt-patch-error"
          className="text-[13px] text-[var(--color-straw)]"
        >
          {patchErrorMessage(patch.error)}
        </p>
      )}
      <p className="text-[11px] font-mono text-mut">
        Changes apply to the next recording.
      </p>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      {/* ─── Cloud card ─────────────────────────────────────────────── */}
      <article
        data-testid="cloud-stt-card"
        className={
          "rounded-xl p-5 space-y-3 bg-white transition-colors " +
          (!isLocal
            ? "border-[1.5px] border-[var(--color-blue)]"
            : "border border-line")
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
          Real-time partials, ~2s end-to-end via Deepgram. Audio is sent to
          the provider.
        </p>

        <div className="border-t border-line pt-3 space-y-2">
          <div className="text-[10px] font-mono uppercase tracking-[0.06em] text-grey">
            DEEPGRAM API KEY · stored locally
          </div>
          {q.data.deepgram_key_masked ? (
            <div className="flex items-center gap-2 text-[12.5px] font-mono">
              <span className="text-ink">{q.data.deepgram_key_masked}</span>
              <span className="text-[var(--color-matcha)] font-semibold">
                ✓ stored
              </span>
            </div>
          ) : (
            <div className="text-sm text-mut">No key stored yet.</div>
          )}
          <div className="flex items-center gap-2 pt-1">
            <input
              type="password"
              placeholder="Paste key…"
              className="flex-1 font-mono text-sm border border-line rounded px-2 py-1.5 focus:border-[var(--color-blue)] outline-none"
              value={keyDraft}
              onChange={(e) => setKeyDraft(e.target.value)}
            />
            <button
              type="button"
              disabled={!keyDraft || setSttKey.isPending}
              className="text-sm font-semibold bg-[var(--color-blue)] text-white px-3 py-1.5 rounded-md disabled:opacity-50"
              onClick={() => setSttKey.mutate(keyDraft)}
            >
              {setSttKey.isPending ? "Saving…" : "Save key"}
            </button>
          </div>
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
    </div>
  );
}
