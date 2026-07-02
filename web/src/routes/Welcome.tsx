/**
 * Phase 7 Plan 07-04 — `/welcome` onboarding route (PRD §5.10).
 *
 * Two-column layout, paper-left / white-right:
 *
 *   ┌──────────────────────────────────┬────────────────────────────────────┐
 *   │  yogurt logo + serif wordmark    │  ONE-TIME SETUP (11px mono caps)   │
 *   │                                   │                                    │
 *   │  "Welcome to yogurt." (52px)     │  [Step 1: Screen Recording]        │
 *   │  Two streams, one set of notes…  │  [Step 2: Microphone]              │
 *   │                                   │  [Step 3: Connect your model]      │
 *   │  <TerminalMockup />              │  [Step 4: Pick transcription]      │
 *   │                                   │  [Take me to my meetings →]        │
 *   │                                   │  "Restart once after granting…"    │
 *   └──────────────────────────────────┴────────────────────────────────────┘
 *
 * Ready-gating: the primary CTA is enabled only when Screen Recording is
 * granted AND Microphone is granted AND at least one provider is active.
 * On click it flips `first_run_completed=true` (Phase 7 backend field
 * added in this plan) and navigates to `/`. The `useFirstRunRedirect`
 * hook then keeps the SPA on `/` for subsequent visits.
 *
 * Quick task 260628-g71 inserted Step 2 (Microphone) and renumbered the
 * provider + transcription cards from 2/3 to 3/4. The mic StepCard's
 * Grant button is the user-facing trigger for `POST /api/audio/microphone/request`
 * — when status is `not_determined`, clicking it fires the macOS system
 * dialog; when status is `denied`, clicking it deep-links into
 * System Settings (macOS won't re-prompt after a denial).
 *
 * Quick task 260701-vjb made Steps 1, 3, and 4 actionable: Step 1's Grant
 * button hits `POST /api/audio/screen-recording/request` (with a System
 * Settings deep link alongside), Steps 3 and 4 link to `/settings`, and
 * Step 4's state is derived from steps 1-3 instead of hardcoded pending.
 */

import {
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";
import { Link, useNavigate } from "react-router";
import { Logo } from "../components/Logo";
import { StepCard, type StepState } from "../components/onboarding/StepCard";
import { TerminalMockup } from "../components/onboarding/TerminalMockup";
import {
  permissionsKey,
  requestMicrophonePermission,
  requestScreenRecordingPermission,
} from "../lib/api/audio";
import {
  useSettings,
  useSetFirstRunCompleted,
} from "../lib/api/settings";
import { useMicrophoneStatus } from "../hooks/useMicrophoneStatus";
import { useScreenRecordingStatus } from "../hooks/useScreenRecordingStatus";

const PROVIDER_CHIPS = [
  { id: "minimax", label: "Minimax" },
  { id: "ollama", label: "Ollama" },
  { id: "openai", label: "OpenAI" },
  { id: "openrouter", label: "OpenRouter" },
];

export function Welcome() {
  const nav = useNavigate();
  const qc = useQueryClient();
  const settings = useSettings();
  const { granted } = useScreenRecordingStatus();
  const { granted: micGranted, status: micStatus } = useMicrophoneStatus();
  const setFirstRun = useSetFirstRunCompleted();

  const micRequest = useMutation({
    mutationFn: requestMicrophonePermission,
    // The POST returns the *current* (likely still `not_determined`)
    // snapshot — the real state change arrives via the 2s poll. Invalidate
    // here so the next render reads the freshest cached value rather
    // than waiting up to 2s for the periodic refetch.
    onSuccess: () => qc.invalidateQueries({ queryKey: permissionsKey }),
  });

  const screenRequest = useMutation({
    mutationFn: requestScreenRecordingPermission,
    onSuccess: () => qc.invalidateQueries({ queryKey: permissionsKey }),
  });

  const handleGrantMic = () => {
    if (micStatus === "denied") {
      // macOS will not re-prompt after a denial — deep-link straight
      // into the Privacy_Microphone pane instead. Exact casing matters;
      // see `<MicPermissionDenied />` doc-comment.
      window.location.href =
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";
    } else {
      micRequest.mutate();
    }
  };

  const providers = settings.data?.providers ?? [];
  const activeProvider = providers.find((p) => p.is_active);
  const hasProvider = !!activeProvider;
  // Best-effort label match against the chip list — the providers row
  // doesn't carry a `preset_id`, so we normalize on the name.
  const activeChipId =
    PROVIDER_CHIPS.find((c) =>
      activeProvider?.name.toLowerCase().includes(c.id),
    )?.id ?? null;

  // Sequential gating mirrors PRD §5.10 — each step is only `current`
  // once its prerequisites are satisfied. The user works through them
  // top-to-bottom (Welcome's existing pattern; we preserve it).
  const step1State: StepState = granted ? "done" : "current";
  const step2State: StepState = micGranted
    ? "done"
    : !granted
      ? "pending"
      : "current";
  const step3State: StepState =
    !granted || !micGranted
      ? "pending"
      : hasProvider
        ? "done"
        : "current";
  // 260701-vjb: Step 4 becomes current once steps 1-3 are done. The
  // transcription choice does not gate `ready` - Deepgram cloud is the
  // seeded default, so this step is informational-but-actionable.
  const step4State: StepState =
    granted && micGranted && hasProvider ? "current" : "pending";

  const ready = granted && micGranted && hasProvider;

  const handleProceed = async () => {
    if (!ready || setFirstRun.isPending) return;
    try {
      await setFirstRun.mutateAsync(true);
      nav("/", { replace: true });
    } catch (e) {
      console.error("first_run_completed flip failed", e);
    }
  };

  // Whether to render the inline Grant button under the mic StepCard.
  // Hidden once permission is positively resolved (granted on macOS,
  // not_required off macOS); shown for both `not_determined` (Grant)
  // and `denied` (Open System Settings).
  const showMicButton =
    micStatus !== "granted" && micStatus !== "not_required";

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
          >
            {!granted ? (
              // Unlike the mic path, CGPreflight cannot distinguish
              // never-asked from denied - so we always show BOTH the Grant
              // button (fires the prompt if never asked) and the System
              // Settings link (recovery after a denial).
              <div className="flex items-center gap-3">
                <button
                  type="button"
                  onClick={() => screenRequest.mutate()}
                  disabled={screenRequest.isPending}
                  className="px-3 py-1.5 rounded-button text-[12.5px] font-semibold text-white bg-blue shadow-button-blue hover:opacity-90 disabled:opacity-50"
                >
                  Grant Screen Recording
                </button>
                <button
                  type="button"
                  onClick={() => {
                    window.location.href =
                      "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";
                  }}
                  className="text-[12.5px] text-blue hover:underline"
                >
                  Open System Settings
                </button>
              </div>
            ) : null}
          </StepCard>

          <StepCard
            number={2}
            title="Microphone"
            state={step2State}
            body="This is how yogurt hears your voice. macOS will show a permission prompt."
          >
            {showMicButton ? (
              <button
                type="button"
                onClick={handleGrantMic}
                disabled={micRequest.isPending}
                className="px-3 py-1.5 rounded-button text-[12.5px] font-semibold text-white bg-blue shadow-button-blue hover:opacity-90 disabled:opacity-50"
              >
                {micStatus === "denied"
                  ? "Open System Settings"
                  : "Grant Microphone"}
              </button>
            ) : null}
          </StepCard>

          <StepCard
            number={3}
            title="Connect your model"
            state={step3State}
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
            {step3State === "current" ? (
              <Link
                to="/settings"
                className="inline-block mt-3 text-[12.5px] text-blue hover:underline"
              >
                Set up in Settings →
              </Link>
            ) : null}
          </StepCard>

          <StepCard
            number={4}
            title="Pick transcription"
            state={step4State}
            body="Cloud Deepgram for speed, or fully-local whisper.cpp."
          >
            {step4State === "current" ? (
              <Link
                to="/settings"
                className="text-[12.5px] text-blue hover:underline"
              >
                Choose in Settings →
              </Link>
            ) : null}
          </StepCard>
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
