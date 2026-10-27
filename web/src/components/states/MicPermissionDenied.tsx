/**
 * The "Open System Settings" href must match exactly (casing, query key, scheme) or macOS will not open the Microphone pane:
 * `x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone`
 * System Settings lists the responsible app (the terminal that ran `yogurt start`, or the yogurt binary itself), which the server reports as `owner`.
 * A denied microphone usually recovers on the next attempt to open the input device, but older macOS releases want a relaunch, so the instructions cover both.
 * "Check again" re-fetches the permission query. Reloading the tab restarts nothing.
 */
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { permissionsKey } from "../../lib/api/audio";
import { Button, buttonClassName } from "../Button";
import { useMicrophoneStatus } from "../../hooks/useMicrophoneStatus";

export function MicPermissionDenied() {
  const qc = useQueryClient();
  const [checking, setChecking] = useState(false);
  const { owner } = useMicrophoneStatus();

  const handleCheckAgain = async () => {
    setChecking(true);
    try {
      await qc.refetchQueries({ queryKey: permissionsKey, type: "active" });
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="flex flex-col items-center text-center mt-20 px-6 max-w-2xl mx-auto">
      <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-strsoft text-straw text-[22px] mb-6">
        <span aria-hidden>⚠</span>
        <span className="sr-only">Warning</span>
      </div>

      <h2 className="heading-state mb-3">
        yogurt can&apos;t hear your voice yet
      </h2>

      <p className="text-[15px] text-mut max-w-lg mb-8">
        macOS requires Microphone permission so yogurt can capture your
        own side of the call.
      </p>

      <ol className="text-left text-[14px] text-ink space-y-2 mb-3 list-decimal list-inside">
        <li>
          Open <strong>System Settings → Privacy &amp; Security → Microphone</strong>
        </li>
        <li>
          {owner ? (
            <>
              Find <strong>{owner}</strong>, the app macOS gives yogurt&apos;s
              permissions to, and toggle it on
            </>
          ) : (
            <>
              Find the app that launched yogurt, usually your terminal, and
              toggle it on
            </>
          )}
        </li>
        <li>
          Click "Check again" below. Most macOS versions pick this up
          immediately
        </li>
        <li>
          Still stuck? Relaunch yogurt, since some macOS releases need a
          fresh process
        </li>
      </ol>

      <p className="text-[12px] text-mut mb-6">
        this is a macOS requirement
      </p>

      <div className="flex items-center gap-3">
        <a
          href="x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
          className={buttonClassName("primary")}
        >
          Open System Settings
        </a>
        <Button
          variant="secondary"
          onClick={handleCheckAgain}
          disabled={checking}
        >
          {checking ? "Checking…" : "Check again"}
        </Button>
      </div>
    </div>
  );
}
