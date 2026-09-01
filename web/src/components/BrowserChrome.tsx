import { type ReactNode } from "react";

interface BrowserChromeProps {
  /** URL to show in the centered pill, e.g. "localhost:7878/welcome". */
  url: string;
  children: ReactNode;
  /** Optional className for outer sizing — typically rounded corners + width. */
  className?: string;
}

/**
 * BrowserChrome — fake-Safari window for full-screen mockups (PRD §16.6).
 * Used by marketing screenshots, the onboarding "boot sequence" preview,
 * and the style-guide mockup gallery.
 *
 * Layout: 42px header on a paper-warm bg (#F4EEE3) with three traffic-light
 * dots on the left and a centered URL pill. Body is whatever children render.
 */
export function BrowserChrome({ url, children, className = "" }: BrowserChromeProps) {
  return (
    <div
      className={[
        "overflow-hidden rounded-card border border-line bg-card",
        "shadow-[0_26px_60px_-28px_rgba(40,30,15,0.4)]",
        className,
      ]
        .join(" ")
        .trim()}
    >
      {/* Header — purely decorative chrome (traffic dots + fake URL pill).
         The entire header is aria-hidden so screenreaders don't announce
         the mock URL as if it were real navigation. (MD-08) */}
      <div
        data-testid="browser-chrome-header"
        className="h-[42px] flex items-center px-4 border-b border-line"
        style={{ background: "color-mix(in srgb, var(--color-line) 55%, var(--color-paper))" }}
        aria-hidden="true"
      >
        {/* Traffic-light dots */}
        <div className="flex items-center gap-2">
          <span
            data-testid="traffic-dot"
            className="inline-block w-3 h-3 rounded-pill"
            style={{ background: "#FF5F57" }}
            aria-hidden
          />
          <span
            data-testid="traffic-dot"
            className="inline-block w-3 h-3 rounded-pill"
            style={{ background: "#FEBC2E" }}
            aria-hidden
          />
          <span
            data-testid="traffic-dot"
            className="inline-block w-3 h-3 rounded-pill"
            style={{ background: "#28C840" }}
            aria-hidden
          />
        </div>

        {/* Centered URL pill */}
        <div className="flex-1 flex justify-center">
          <span
            className={[
              "inline-block rounded-pill px-3 py-1",
              "bg-card border border-line",
              "font-mono text-[11px] text-mut",
            ].join(" ")}
          >
            {url}
          </span>
        </div>

        {/* Right-side spacer to keep URL truly centered (mirrors dot width) */}
        <div className="w-[52px]" aria-hidden />
      </div>

      {/* Body */}
      <div className="bg-paper">{children}</div>
    </div>
  );
}
