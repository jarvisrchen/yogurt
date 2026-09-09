import { useEffect, useState } from "react";
import { Check, Link2 } from "lucide-react";

/**
 * Icon button that copies the current page URL (the meeting's live or
 * post-meeting URL, whichever route this renders in) to the clipboard,
 * with a ~1.5s checkmark swap as confirmation.
 *
 * Mirrors the icon-button styling of `DeleteMeetingConfirm`'s `icon`
 * variant. No toast exists anywhere in this app to reuse (see
 * `copyMeetingMarkdown`'s doc comment for the aspirational one that was
 * never built) — the icon swap is a self-contained substitute.
 */
export function CopyUrlButton({ className = "" }: { className?: string }) {
  const [copied, setCopied] = useState(false);

  // Auto-revert lives in an effect (not the click handler) so unmounting
  // mid-flight - e.g. clicking Copy on the live Meeting view then
  // immediately End meeting, which navigates away - clears the timer
  // instead of firing against a discarded instance.
  useEffect(() => {
    if (!copied) return;
    const t = setTimeout(() => setCopied(false), 1500);
    return () => clearTimeout(t);
  }, [copied]);

  const onClick = async () => {
    await navigator.clipboard.writeText(window.location.href);
    setCopied(true);
  };

  return (
    <button
      type="button"
      aria-label="Copy meeting URL"
      title="Copy meeting URL"
      onClick={onClick}
      className={`shrink-0 p-1.5 rounded-button focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue/40 ${
        copied ? "text-matcha" : "text-mut hover:text-ink hover:bg-line/40"
      } ${className}`}
    >
      {copied ? (
        <Check size={16} aria-hidden />
      ) : (
        <Link2 size={16} aria-hidden />
      )}
    </button>
  );
}
