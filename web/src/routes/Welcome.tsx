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
import { useModels } from "../lib/api/stt";
import { useMicrophoneStatus } from "../hooks/useMicrophoneStatus";
import { useScreenRecordingStatus } from "../hooks/useScreenRecordingStatus";
import { LABEL } from "../components/settings/labelClass";

const PROVIDER_CHIPS = [
  { id: "lm studio", label: "LM Studio" },
  { id: "minimax", label: "Minimax" },
  { id: "ollama", label: "Ollama" },
  { id: "openai", label: "OpenAI" },
  { id: "openrouter", label: "OpenRouter" },
];

/** Backend error bodies are not user copy, so the raw string sits behind a `<details>`. */
function ErrorNote({
  error,
  friendly,
}: {
  error: unknown;
  friendly: string;
}) {
  if (!error) return null;
  const raw = error instanceof Error ? error.message : String(error);
  return (
    <p className="mt-2 text-[12.5px] text-straw">
      {friendly}
      <details className="mt-1">
        <summary className="cursor-pointer text-[11px] text-mut">
          Details
        </summary>
        <code className="block mt-1 text-[11px] text-mut break-all">
          {raw}
        </code>
      </details>
    </p>
  );
}

export function Welcome() {
  const nav = useNavigate();
  const qc = useQueryClient();
  const settings = useSettings();
  const { granted } = useScreenRecordingStatus();
  const { granted: micGranted, status: micStatus } = useMicrophoneStatus();
  const setFirstRun = useSetFirstRunCompleted();
  const models = useModels();

  const micRequest = useMutation({
    mutationFn: requestMicrophonePermission,
    // The POST returns the current snapshot, usually still `not_determined`.
    // Invalidate so the 2s poll doesn't have to catch up.
    onSuccess: () => qc.invalidateQueries({ queryKey: permissionsKey }),
  });

  const screenRequest = useMutation({
    mutationFn: requestScreenRecordingPermission,
    onSuccess: () => qc.invalidateQueries({ queryKey: permissionsKey }),
  });

  const handleGrantMic = () => {
    if (micStatus === "denied") {
      // macOS will not re-prompt after a denial, so deep-link to the
      // Privacy_Microphone pane. Exact casing matters.
      window.location.href =
        "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";
    } else {
      micRequest.mutate();
    }
  };

  const providers = settings.data?.providers ?? [];
  const activeProvider = providers.find((p) => p.is_active);
  const hasProvider = !!activeProvider;
  // The providers row has no `preset_id`, so match on the normalized name.
  const activeChipId =
    PROVIDER_CHIPS.find((c) =>
      activeProvider?.name.toLowerCase().includes(c.id),
    )?.id ?? null;

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
  const general = settings.data?.general;
  const sttUsable =
    general?.stt_provider === "local"
      ? !!models.data?.find((m) => m.name === general.stt_model)?.downloaded
      : !!settings.data?.deepgram_key_masked;
  const step4State: StepState =
    !granted || !micGranted || !hasProvider
      ? "pending"
      : sttUsable
        ? "done"
        : "current";

  // Transcription does not gate `ready`: Deepgram cloud is the seeded default.
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

  const showMicButton =
    micStatus !== "granted" && micStatus !== "not_required";

  return (
    <div className="grid grid-cols-[1.05fr_0.95fr] min-h-screen">
      <section className="bg-paper px-16 py-16 flex flex-col justify-center">
        <div className="flex items-center gap-3 mb-12">
          <Logo size={36} />
          <span className="heading-section">
            yogurt
          </span>
        </div>

        <h1 className="heading-hero mb-4">
          Welcome to yogurt.
        </h1>
        <p className="text-[15px] text-mut max-w-md mb-10">
          Yogurt hears both sides of the call without joining it, and folds
          what was said into the notes you type. All of it runs on this Mac.
        </p>

        <div className="max-w-md">
          <TerminalMockup />
        </div>
      </section>

      <section className="bg-card px-12 py-16 flex flex-col">
        <p className={`${LABEL} mb-4`}>
          ONE-TIME SETUP
        </p>

        <div className="flex flex-col gap-4">
          <StepCard
            number={1}
            title="Screen Recording"
            state={step1State}
            body="This is how yogurt hears the other side of the call, without joining it as a bot."
          >
            {!granted ? (
              <>
                {/* CGPreflight cannot tell never-asked from denied, so show both
                    the Grant button and the System Settings link. */}
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
                <p className="mt-2 text-[12px] text-mut">
                  Granting this quits yogurt immediately. That is macOS
                  doing it, not a crash. Relaunch yogurt and reload this
                  page; the steps below will pick up where you left off.
                </p>
                <ErrorNote
                  error={screenRequest.isError ? screenRequest.error : null}
                  friendly="Couldn't reach the permission prompt."
                />
              </>
            ) : null}
          </StepCard>

          <StepCard
            number={2}
            title="Microphone"
            state={step2State}
            body="This is how yogurt hears your voice. macOS will show a permission prompt."
          >
            {showMicButton ? (
              <>
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
                <ErrorNote
                  error={micRequest.isError ? micRequest.error : null}
                  friendly="Couldn't reach the permission prompt."
                />
              </>
            ) : null}
          </StepCard>

          <StepCard
            number={3}
            title="Connect your model"
            state={step3State}
            body="Bring your own key for any OpenAI-compatible provider. Nothing is built in."
          >
            <div className="flex flex-wrap gap-2">
              {PROVIDER_CHIPS.map((c) => {
                const isActive = activeChipId === c.id;
                return (
                  <span
                    key={c.id}
                    className={
                      isActive
                        ? "px-2.5 py-1 rounded-pill text-[12px] font-medium bg-blsoft text-blue border border-blue"
                        : "px-2.5 py-1 rounded-pill text-[12px] font-medium border border-dashed border-line text-mut"
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
                to="/settings/transcription"
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
        <ErrorNote
          error={setFirstRun.isError ? setFirstRun.error : null}
          friendly="Couldn't save that. Your permissions and provider are still set, so try again."
        />

        <p className="mt-4 text-[13px] text-mut text-center">
          Restart once after granting. That one is a macOS quirk.
        </p>
      </section>
    </div>
  );
}
