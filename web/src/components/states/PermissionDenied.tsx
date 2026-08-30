/**
 * Phase 7 (Plan 07-04) — PermissionDenied state (STATE-02).
 *
 * Rendered by `<Library />` (and any in-meeting route that needs system
 * audio) when `GET /api/audio/permission` reports `denied`. PRD §5.11 + the
 * `useScreenRecordingStatus` hook drive the gating.
 *
 * The "Open System Settings" anchor's `href` is a load-bearing string:
 *
 *   `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`
 *
 * Do NOT alter the casing, query-string key, or scheme — macOS only opens
 * the Screen Recording pane when the URI matches exactly. The plan's
 * acceptance criteria grep this file verbatim.
 *
 * Honesty pass (fast task) — this card used to say "Toggle Yogurt on" and
 * ship a "Restart Yogurt" button that just called `window.location.reload()`.
 * Both were wrong:
 *
 *   - The System Settings list shows whichever process actually asked for
 *     Screen Recording — the yogurt binary when launched directly (e.g.
 *     `just release`), or Terminal/iTerm when launched via `just dev`
 *     (the shell is the requesting process, not "Yogurt"). There is no
 *     single toggle labeled "Yogurt" to find.
 *   - Reloading the browser tab does nothing: the backend process is what
 *     needs the TCC grant, and macOS only re-reads it on that process's
 *     *next launch* (see `crates/yogurt-audio/src/permission.rs`). Worse,
 *     granting the permission causes macOS to immediately quit the
 *     process that asked for it — so by the time the user is back here,
 *     the server may already be down and reload would just hit a dead
 *     connection.
 *
 * Replaced with an honest flow: explicit copy about which app to look
 * for and that granting quits the process, static relaunch instructions
 * (no button can relaunch a process from inside the browser sandbox),
 * and a "Check again" button that re-fetches the permission query instead
 * of reloading the page — useful once the user has actually relaunched.
 */
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { permissionsKey } from "../../lib/api/audio";
import { Button, buttonClassName } from "../Button";

export function PermissionDenied() {
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

      <h2 className="heading-state mb-3">
        Yogurt can&apos;t hear the call yet
      </h2>

      <p className="text-[15px] text-mut max-w-lg mb-8">
        macOS requires Screen Recording permission so Yogurt can listen to
        the other side of the call without joining as a bot.
      </p>

      <ol className="text-left text-[14px] text-ink space-y-2 mb-3 list-decimal list-inside">
        <li>
          Open <strong>System Settings → Privacy &amp; Security → Screen
          Recording</strong>
        </li>
        <li>
          Find the app that launched yogurt. That is{" "}
          <strong>Terminal</strong> or <strong>iTerm</strong> if you started
          it from the command line, or <strong>yogurt</strong> itself if you
          opened it directly. Toggle it on
        </li>
        <li>
          macOS will immediately quit that process. That is expected
        </li>
        <li>
          Relaunch yogurt, then come back here and check again
        </li>
      </ol>

      <p className="text-[12px] font-mono text-mut mb-6">
        this is a macOS requirement
      </p>

      <div className="flex items-center gap-3">
        <a
          href="x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
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
