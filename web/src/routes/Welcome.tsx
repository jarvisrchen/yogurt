/**
 * Phase 7 Plan 07-04 — `/welcome` onboarding route (PRD §5.10).
 *
 * Two-column layout, paper-left / white-right:
 *
 *   ┌──────────────────────────────────┬────────────────────────────────────┐
 *   │  yogurt logo + serif wordmark    │  ONE-TIME SETUP (11px mono caps)   │
 *   │                                   │                                    │
 *   │  "Welcome to yogurt." (52px)     │  [Step 1: Screen Recording]        │
 *   │  Two streams, one set of notes…  │  [Step 2: Connect your model]      │
 *   │                                   │  [Step 3: Pick transcription]      │
 *   │  <TerminalMockup />              │                                    │
 *   │                                   │  [Take me to my meetings →]        │
 *   │                                   │  "Restart once after granting…"    │
 *   └──────────────────────────────────┴────────────────────────────────────┘
 *
 * Ready-gating: the primary CTA is enabled only when Screen Recording is
 * granted AND at least one provider is active. On click it flips
 * `first_run_completed=true` (Phase 7 backend field added in this plan)
 * and navigates to `/`. The `useFirstRunRedirect` hook then keeps the
 * SPA on `/` for subsequent visits.
 */

import { useNavigate } from "react-router";
import { Logo } from "../components/Logo";
import { StepCard, type StepState } from "../components/onboarding/StepCard";
import { TerminalMockup } from "../components/onboarding/TerminalMockup";
import {
  useSettings,
  useSetFirstRunCompleted,
} from "../lib/api/settings";
import { useScreenRecordingStatus } from "../hooks/useScreenRecordingStatus";

const PROVIDER_CHIPS = [
  { id: "minimax", label: "Minimax" },
  { id: "ollama", label: "Ollama" },
  { id: "openai", label: "OpenAI" },
  { id: "openrouter", label: "OpenRouter" },
];

export function Welcome() {
  const nav = useNavigate();
  const settings = useSettings();
  const { granted } = useScreenRecordingStatus();
  const setFirstRun = useSetFirstRunCompleted();

  const providers = settings.data?.providers ?? [];
  const activeProvider = providers.find((p) => p.is_active);
  const hasProvider = !!activeProvider;
  // Best-effort label match against the chip list — the providers row
  // doesn't carry a `preset_id`, so we normalize on the name.
  const activeChipId =
    PROVIDER_CHIPS.find((c) =>
      activeProvider?.name.toLowerCase().includes(c.id),
    )?.id ?? null;

  const step1State: StepState = granted ? "done" : "current";
  const step2State: StepState = !granted
    ? "pending"
    : hasProvider
      ? "done"
      : "current";
  const step3State: StepState = "pending";

  const ready = granted && hasProvider;

  const handleProceed = async () => {
    if (!ready || setFirstRun.isPending) return;
    try {
      await setFirstRun.mutateAsync(true);
      nav("/", { replace: true });
    } catch (e) {
      console.error("first_run_completed flip failed", e);
    }
  };

  return (
    <div className="grid grid-cols-[1.05fr_0.95fr] min-h-screen">
      {/* ─── Left: brand + serif headline + terminal mockup ─────────── */}
      <section className="bg-paper px-16 py-16 flex flex-col justify-center">
        <div className="flex items-center gap-3 mb-12">
          <Logo size={36} />
          <span className="font-serif text-[26px] text-ink leading-none">
            yogurt
          </span>
        </div>

        <h1 className="font-serif text-[52px] leading-[1.05] tracking-tight text-ink mb-4">
          Welcome to yogurt.
        </h1>
        <p className="text-[15px] text-mut max-w-md mb-10">
          Two streams, one set of notes, zero bots in the call. Everything
          below happens on this Mac.
        </p>

        <div className="max-w-md">
          <TerminalMockup />
        </div>
      </section>

      {/* ─── Right: ONE-TIME SETUP card stack + primary CTA ─────────── */}
      <section className="bg-card px-12 py-16 flex flex-col">
        <p className="text-[11px] font-mono uppercase tracking-wider text-mut mb-4">
          ONE-TIME SETUP
        </p>

        <div className="flex flex-col gap-4">
          <StepCard
            number={1}
            title="Screen Recording"
            state={step1State}
            body="This is how yogurt hears the other side of the call — no meeting bot required."
          />

          <StepCard
            number={2}
            title="Connect your model"
            state={step2State}
            body="Bring your own key — OpenAI-compatible. Nothing is built in."
          >
            <div className="flex flex-wrap gap-2">
              {PROVIDER_CHIPS.map((c) => {
                const isActive = activeChipId === c.id;
                return (
                  <span
                    key={c.id}
                    className={
                      isActive
                        ? "px-2.5 py-1 rounded-pill text-[12px] font-mono bg-blsoft text-blue border border-blue"
                        : "px-2.5 py-1 rounded-pill text-[12px] font-mono border border-dashed border-line text-mut"
                    }
                  >
                    {c.label}
                  </span>
                );
              })}
            </div>
          </StepCard>

          <StepCard
            number={3}
            title="Pick transcription"
            state={step3State}
            body="Cloud Deepgram for speed, or fully-local whisper.cpp."
          />
        </div>

        <button
          type="button"
          onClick={handleProceed}
          disabled={!ready || setFirstRun.isPending}
          className={
            ready
              ? "mt-10 w-full py-3 rounded-button text-[14px] font-semibold text-white bg-blue shadow-button-blue hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40 focus-visible:ring-offset-2 focus-visible:ring-offset-card"
              : "mt-10 w-full py-3 rounded-button text-[14px] font-semibold text-white bg-blue/40 shadow-button-blue cursor-not-allowed"
          }
        >
          Take me to my meetings →
        </button>

        <p className="mt-4 text-[12px] font-mono text-mut text-center">
          Restart once after granting — a macOS quirk, not us.
        </p>
      </section>
    </div>
  );
}
