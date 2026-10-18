import { type ReactNode } from "react";

interface BrowserChromeProps {
  url: string;
  children: ReactNode;
  className?: string;
}

export function BrowserChrome({ url, children, className = "" }: BrowserChromeProps) {
  return (
    <div
      className={[
        "overflow-hidden rounded-card border border-line bg-card",
        "shadow-window",
        className,
      ]
        .join(" ")
        .trim()}
    >
      {/* aria-hidden so screenreaders don't announce the mock URL as navigation. */}
      <div
        data-testid="browser-chrome-header"
        className="h-[42px] flex items-center px-4 border-b border-line"
        style={{ background: "color-mix(in srgb, var(--color-line) 55%, var(--color-paper))" }}
        aria-hidden="true"
      >
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

        <div className="flex-1 flex justify-center">
          <span
            className={[
              "inline-block rounded-pill px-3 py-1",
              "bg-card border border-line",
              "text-[11px] text-mut",
            ].join(" ")}
          >
            {url}
          </span>
        </div>

        <div className="w-[52px]" aria-hidden />
      </div>

      <div className="bg-paper">{children}</div>
    </div>
  );
}
