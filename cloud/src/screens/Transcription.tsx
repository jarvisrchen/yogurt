import { useEffect, useState } from "react";
import { Button } from "@web/components/Button";
import { Pill } from "@web/components/Pill";
import { ModelPicker } from "@web/components/settings/ModelPicker";
import { LABEL } from "@web/components/settings/labelClass";
import type { ModelView } from "@web/components/settings/modelTypes";
import { Back, LockButton, useDesktop } from "../ui";
import { phoneModels, readStt, saveStt, type PhoneModel, type SttChoice } from "../lib/stt";

const MB = (b: number) => `${Math.round(b / 1e6)} MB`;
const card = (active: boolean) => `rounded-card bg-card p-5 space-y-4 ${active ? "border-[1.5px] border-blue shadow-button-blue" : "border border-line"}`;
const inUse = (
  <Pill tone="matcha" className="shrink-0">
    <span className="inline-block w-[7px] h-[7px] rounded-pill bg-matcha" aria-hidden />
    In use
  </Pill>
);

type Verdict = { ok: true; model: string } | { ok: false; error: string };

/**
 * The Mac's Transcription settings for a phone: its Cloud and Local cards and model pills, the same classes at the webapp's phone sizes.
 * Local runs only in the Android app, whose bridge downloads the models. A browser shows the card disabled with the reason.
 */
export function Transcription({ onBack, onLock }: { onBack: () => void; onLock: () => void }) {
  const desktop = useDesktop();
  const bridge = window.YogurtAndroid;
  const [choice, setChoice] = useState<SttChoice>(readStt);
  const [models, setModels] = useState<PhoneModel[]>(phoneModels);
  const [error, setError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [verdict, setVerdict] = useState<{ name: string; v: Verdict } | null>(null);
  const busy = models.some((m) => m.downloading);

  // The app reports a download's progress only when asked.
  useEffect(() => {
    if (!busy) return;
    const id = window.setInterval(() => setModels(phoneModels()), 1000);
    return () => window.clearInterval(id);
  }, [busy]);

  useEffect(() => {
    const on = (e: Event) => {
      const raw = (e as CustomEvent).detail;
      const d = typeof raw === "string" ? JSON.parse(raw) : raw;
      if (d.type !== "model-test") return;
      setTesting(false);
      setVerdict({ name: d.name, v: d.ok ? { ok: true, model: d.model } : { ok: false, error: d.error } });
    };
    window.addEventListener("yogurt-native", on);
    return () => window.removeEventListener("yogurt-native", on);
  }, []);

  const choose = (c: SttChoice) => {
    saveStt(c);
    setChoice(c);
  };
  const answer = (json: string) => {
    const r = JSON.parse(json);
    setError(r.ok ? null : r.error);
    setModels(phoneModels());
  };
  const local = !!bridge && choice.engine === "local";
  const chosen = models.find((m) => m.name === choice.model && m.downloaded);
  // Use Local takes the chosen model, or else the first one on the phone. While Local is in use, only the chosen one counts.
  const usable = chosen ?? (local ? undefined : models.find((m) => m.downloaded));
  const blocked = !bridge || !usable;
  const loading = models.find((m) => m.downloading);
  const failed = models.find((m) => m.error);

  return (
    <div className="min-h-dvh flex flex-col">
      {desktop && (
        <div className="flex items-center justify-between gap-[10px] px-[20px] py-[12px] bg-card border-b border-line">
          <Back to="Settings" onClick={onBack} />
          <LockButton onClick={onLock} />
        </div>
      )}
      <main className="cloud-stt w-full max-w-[640px] mx-auto flex flex-col gap-[10px] px-[16px] pt-[16px] pb-[16px] min-[721px]:px-[24px] min-[721px]:pt-[22px] min-[721px]:pb-[28px]">
        {!desktop && <Back to="Settings" onClick={onBack} />}
        <h1 className="text-[22px] font-extrabold tracking-[-0.01em] text-ink m-[0px] min-[721px]:mb-[2px]">Transcription</h1>
        <p className="text-[13px] min-[721px]:text-[12.5px] text-mut m-[0px]">Changes apply to the next recording.</p>

        <article data-testid="cloud-stt-card" className={card(!local)}>
          <header className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2 min-w-0">
              <h3 className="heading-card truncate">Cloud</h3>
              <Pill tone="neutral">Deepgram</Pill>
            </div>
            {local ? (
              <Button variant="primary" className="px-3 py-1.5 text-[13px] whitespace-nowrap shrink-0" onClick={() => choose({ ...choice, engine: "cloud" })}>
                Use Cloud
              </Button>
            ) : (
              inUse
            )}
          </header>
          <p className="text-[12.5px] text-mut">Real-time partials, ~2s end-to-end via Deepgram. Audio is sent to the provider.</p>
          <div className="border-t border-line pt-4 mt-4 space-y-2">
            <div className={LABEL}>Deepgram key · in your Vercel project</div>
            <div className="font-mono text-[13.5px] text-ink break-all">YOGURT_DEEPGRAM_API_KEY</div>
            <p className="text-[12.5px] text-mut">The phone gets a token that works for 30 seconds, for each connection.</p>
          </div>
        </article>

        <article data-testid="local-stt-card" className={card(local)}>
          <header className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2 min-w-0">
              <h3 className="heading-card whitespace-nowrap">Local</h3>
              <Pill tone="neutral">sherpa-onnx</Pill>
            </div>
            {local ? (
              inUse
            ) : (
              <Button
                variant="primary"
                className="px-3 py-1.5 text-[13px] whitespace-nowrap shrink-0"
                disabled={blocked}
                onClick={() => usable && choose({ engine: "local", model: usable.name })}
              >
                Use Local
              </Button>
            )}
          </header>
          {bridge && blocked && !local && <span className="text-[11px] text-straw">Download a model first</span>}
          {local && !chosen && <span className="text-[11px] text-straw">{choice.model} isn't on this phone. Pick a downloaded model, or use Cloud.</span>}
          <p className="text-[12.5px] text-mut">
            {bridge
              ? "Runs on this phone's processor. Audio never leaves this phone. Local is free."
              : "Needs the Yogurt app for Android. A page in a browser stops when you switch apps, and the model with it."}
          </p>
          {bridge && (
            <>
              <ModelPicker
                models={models.map((m) => ({ name: m.name as ModelView["name"], size_mb: m.size_mb, downloaded: m.downloaded, downloading: m.downloading, intel_supported: true, managed_by_homebrew: false }))}
                selected={usable?.name ?? null}
                onSelect={(name) => choose({ engine: "local", model: name })}
                onRequestDownload={(name) => !models.find((m) => m.name === name)?.downloading && answer(bridge.downloadModel(name))}
                activeDownloadName={loading?.name ?? null}
                activeDownloadProgress={loading ? { bytesDownloaded: loading.bytes, totalBytes: loading.total, bytesPerSec: 0, etaSeconds: null, complete: false, error: null } : null}
                onDelete={(name) => answer(bridge.deleteModel(name))}
                activeModelName={local ? usable?.name ?? null : null}
              />
              {loading && (
                <p role="status" className="text-[12.5px] text-mut">
                  {loading.bytes >= loading.total ? `Checking ${loading.name}…` : `Downloading ${loading.name} · ${MB(loading.bytes)} of ${MB(loading.total)}${loading.waiting ? ` · ${loading.waiting}` : ""}`}{" "}
                  <button type="button" onClick={() => (bridge.cancelDownload(loading.name), setModels(phoneModels()))} className="bg-transparent border-0 p-[0px] font-[inherit] text-[inherit] text-blue cursor-pointer">
                    Cancel
                  </button>
                </p>
              )}
              {failed && <p role="alert" className="text-[12.5px] text-straw">{failed.name}: {failed.error}</p>}
              {error && <p role="alert" className="text-[12.5px] text-straw">{error}</p>}
              <div className="flex flex-wrap items-center gap-2 border-t border-line pt-4 mt-4">
                <Button
                  variant="secondary"
                  disabled={!usable || testing}
                  aria-label={`Test connection for ${usable?.name ?? "the model"}`}
                  className="shrink-0 px-3 py-1.5 text-[13px]"
                  onClick={() => usable && (setTesting(true), setVerdict(null), bridge.testModel(usable.name))}
                >
                  {testing ? "Testing…" : "Test"}
                </Button>
                {verdict && verdict.name === usable?.name && (
                  <p role="status" className={`order-last basis-full text-[12.5px] ${verdict.v.ok ? "text-matcha" : "text-straw"}`}>
                    {verdict.v.ok ? `✓ Connection works · answered as ${verdict.v.model}` : `✗ ${verdict.v.error}`}
                  </p>
                )}
              </div>
              <p className="text-[12.5px] text-mut pt-1">Tap a model to download it · stored in the app on this phone. Parakeet is by NVIDIA, CC-BY-4.0.</p>
            </>
          )}
        </article>
      </main>
    </div>
  );
}
