// The Cloud card is Deepgram only because the server supports no other cloud STT.
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { settingsApi, type General } from "../../lib/api/settings";
import { LocalSTTCard } from "./LocalSTTCard";
import { TestKeyButton } from "./TestKeyButton";
import { Button } from "../Button";
import { Pill } from "../Pill";
import { LABEL } from "./labelClass";

// http() throws "<status> <statusText>: <body>". The 422 body is {"error": "<msg>"},
// so show just the message and fall back to the raw text for any other shape.
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
      <p className="text-[13px] text-mut">Loading transcription…</p>
    );
  }
  const general = q.data.general;
  const isLocal = general.stt_provider === "local";
  const selectedModel = general.stt_model || "small.en";

  return (
    <div className="space-y-3">
      {patch.isError && (
        <p data-testid="stt-patch-error" className="text-[13px] text-straw">
          {patchErrorMessage(patch.error)}
        </p>
      )}
      <p className="text-[12.5px] text-mut">
        Changes apply to the next recording.
      </p>
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
      <article
        data-testid="cloud-stt-card"
        className={
          "rounded-card bg-card p-5 space-y-4 " +
          (!isLocal
            ? "border-[1.5px] border-blue shadow-button-blue"
            : "border border-line")
        }
      >
        <header className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2 min-w-0">
            <h3 className="heading-card truncate">Cloud</h3>
            <Pill tone="neutral">Deepgram</Pill>
          </div>
          {!isLocal ? (
            <Pill tone="matcha" className="shrink-0">
              <span
                className="inline-block w-[7px] h-[7px] rounded-pill bg-matcha"
                aria-hidden
              />
              In use
            </Pill>
          ) : (
            <Button
              variant="primary"
              className="px-3 py-1.5 text-[13px] whitespace-nowrap shrink-0"
              onClick={() => patch.mutate({ stt_provider: "cloud" })}
            >
              Use Cloud
            </Button>
          )}
        </header>
        <p className="text-[12.5px] text-mut">
          Real-time partials, ~2s end-to-end via Deepgram. Audio is sent to
          the provider.
        </p>

        <div className="border-t border-line pt-4 mt-4 space-y-2">
          <div className={LABEL}>
            Deepgram API key · stored locally
          </div>
          {q.data.deepgram_key_masked ? (
            <div className="flex items-center gap-2">
              <span className="text-[13.5px] text-ink">{q.data.deepgram_key_masked}</span>
              <span className="text-matcha text-[12.5px] font-medium">
                ✓ stored
              </span>
            </div>
          ) : (
            <div className="text-[12.5px] text-mut">No key stored yet.</div>
          )}
          <div className="flex flex-wrap items-center gap-2 pt-1">
            <input
              type="password"
              placeholder="Paste key…"
              aria-label="Deepgram API key"
              className="flex-1 rounded-button border border-line bg-paper px-3 py-2 text-[13.5px] font-mono text-ink placeholder:text-grey placeholder:font-sans focus:border-blue focus:outline-none"
              value={keyDraft}
              onChange={(e) => setKeyDraft(e.target.value)}
            />
            <Button
              variant="secondary"
              className="px-3 py-1.5 text-[13px]"
              disabled={!keyDraft || setSttKey.isPending}
              onClick={() => setSttKey.mutate(keyDraft)}
            >
              {setSttKey.isPending ? "Saving…" : "Save key"}
            </Button>
            <TestKeyButton
              providerName="Deepgram"
              draft={keyDraft}
              hasStoredKey={!!q.data.deepgram_key_masked}
              testFn={settingsApi.testSttKey}
            />
          </div>
        </div>
      </article>

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
