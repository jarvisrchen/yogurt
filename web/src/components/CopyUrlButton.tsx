import { useEffect, useState } from "react";
import { Check, Link2 } from "lucide-react";

export function CopyUrlButton({ className = "" }: { className?: string }) {
  const [copied, setCopied] = useState(false);

  // Revert in an effect, not the click handler, so unmounting mid-flight (End meeting navigates away) clears the timer.
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
