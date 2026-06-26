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
 * After granting, macOS requires a process restart for the TCC grant to
 * actually take effect (see `crates/yogurt-audio/src/permission.rs`). The
 * mono caption + "Restart Yogurt" affordance make that quirk explicit.
 */

export function PermissionDenied() {
  return (
    <div className="flex flex-col items-center text-center mt-20 px-6 max-w-2xl mx-auto">
      <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-strsoft text-straw text-[22px] mb-6">
        <span aria-hidden>⚠</span>
        <span className="sr-only">Warning</span>
      </div>

      <h2 className="font-serif text-[34px] text-ink mb-3">
        Yogurt can&apos;t hear the call yet
      </h2>

      <p className="text-[15px] text-mut max-w-lg mb-8">
        macOS requires Screen Recording permission so Yogurt can listen to
        the other side of the call without joining as a bot.
      </p>

      <ol className="text-left text-[14px] text-ink space-y-2 mb-8 list-decimal list-inside">
        <li>
          Open <strong>System Settings → Privacy &amp; Security → Screen
          Recording</strong>
        </li>
        <li>Toggle Yogurt on</li>
        <li>Restart Yogurt once</li>
      </ol>

      <p className="text-[12px] font-mono text-mut mb-6">
        a macOS requirement, not us
      </p>

      <div className="flex items-center gap-3">
        <a
          href="x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
          className="bg-blue text-white text-[13.5px] font-semibold rounded-button px-4 py-2 shadow-button-blue hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40 focus-visible:ring-offset-2 focus-visible:ring-offset-paper"
        >
          Open System Settings
        </a>
        <button
          type="button"
          onClick={() => window.location.reload()}
          className="border border-line text-ink text-[13.5px] font-semibold rounded-button px-4 py-2 hover:bg-line/30 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40 focus-visible:ring-offset-2 focus-visible:ring-offset-paper"
        >
          Restart Yogurt
        </button>
      </div>
    </div>
  );
}
