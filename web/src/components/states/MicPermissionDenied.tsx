/**
 * Quick task 260628-g71 — MicPermissionDenied state.
 *
 * Parallel to `<PermissionDenied />` (which is the Screen-Recording
 * denial card). Rendered by `<Library />` when `useMicrophoneStatus`
 * reports `denied` AND screen-recording is already granted (screen
 * recording denial takes precedence — it's the more fundamental capture
 * path).
 *
 * The "Open System Settings" anchor's `href` is a load-bearing string:
 *
 *   `x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone`
 *
 * Do NOT alter the casing, query-string key, or scheme — macOS only
 * opens the Microphone pane when the URI matches exactly. The plan's
 * acceptance criteria grep this file verbatim.
 *
 * Honesty pass (fast task) — same two lies as `<PermissionDenied />` had:
 * "Toggle Yogurt on" names an app that doesn't appear in the System
 * Settings list (it's whichever process asked — Terminal/iTerm under
 * `just dev`, or the yogurt binary itself when launched directly), and
 * the "Restart Yogurt" button just reloaded the browser tab, which
 * restarts nothing. Unlike screen recording, a denied microphone
 * permission CAN be recovered without a process restart on modern
 * macOS — flipping the toggle takes effect on the next attempt to open
 * the input device — but the long tail of macOS releases still wants a
 * relaunch, so the honest instructions cover both. Replaced the button
 * with "Check again", which re-fetches the permission query instead of
 * reloading the page.
 */
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { permissionsKey } from "../../lib/api/audio";

export function MicPermissionDenied() {
  const qc = useQueryClient();
  const [checking, setChecking] = useState(false);

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

      <h2 className="font-serif text-[34px] text-ink mb-3">
        Yogurt can&apos;t hear your voice yet
      </h2>

      <p className="text-[15px] text-mut max-w-lg mb-8">
        macOS requires Microphone permission so Yogurt can capture what
        YOU say in the call.
      </p>

      <ol className="text-left text-[14px] text-ink space-y-2 mb-3 list-decimal list-inside">
        <li>
          Open <strong>System Settings → Privacy &amp; Security → Microphone</strong>
        </li>
        <li>
          Find the app that launched yogurt — <strong>Terminal</strong> or{" "}
          <strong>iTerm</strong> under{" "}
          <code className="font-mono text-ink">just dev</code>, or{" "}
          <strong>yogurt</strong> itself if you open it directly — and
          toggle it on
        </li>
        <li>Click "Check again" below — most macOS versions pick this up immediately</li>
        <li>
          Still stuck? Relaunch it (
          <code className="font-mono text-ink">just dev</code> again, or
          reopen the app) — some releases need a fresh process
        </li>
      </ol>

      <p className="text-[12px] font-mono text-mut mb-6">
        a macOS requirement, not us
      </p>

      <div className="flex items-center gap-3">
        <a
          href="x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
          className="bg-blue text-white text-[13.5px] font-semibold rounded-button px-4 py-2 shadow-button-blue hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40 focus-visible:ring-offset-2 focus-visible:ring-offset-paper"
        >
          Open System Settings
        </a>
        <button
          type="button"
          onClick={handleCheckAgain}
          disabled={checking}
          className="border border-line text-ink text-[13.5px] font-semibold rounded-button px-4 py-2 hover:bg-line/30 disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40 focus-visible:ring-offset-2 focus-visible:ring-offset-paper"
        >
          {checking ? "Checking…" : "Check again"}
        </button>
      </div>
    </div>
  );
}
