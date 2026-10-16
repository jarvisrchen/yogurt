/**
 * The "Open System Settings" href must match exactly (casing, query key, scheme) or macOS will not open the Screen Recording pane:
 * `x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture`
 * The System Settings list shows whichever process asked (the yogurt binary when launched directly, Terminal/iTerm under `just dev`), so there is no "Yogurt" toggle.
 * Reloading the tab cannot help: macOS re-reads the TCC grant only on the process's next launch (see `crates/yogurt-audio/src/permission.rs`),
 * and granting makes macOS quit the asking process, so the server may already be down.
 * Hence static relaunch instructions (a browser cannot relaunch a process) and a "Check again" button that re-fetches the permission query.
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

      <p className="text-[12px] text-mut mb-6">
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
