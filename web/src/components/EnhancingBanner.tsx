// EnhancingBanner — sticky lilac banner shown while the LLM is enriching
// the user's notes. PRD §5.11 + §16.5, CONTEXT D-28.
//
// Layout (left → right):
//
//   ● Weaving your notes into the transcript…   ▭▭▭▭▭▭▭▭▭▭▭     1,234 chars
//   ↑                                            ↑               ↑
//   pulse dot (1.4s recpulse)                    progress bar    JetBrains
//   blueberry (#5B4FC7)                          (1.8s)          Mono count
//
// Background: --color-blsoft (#ECE9FB) per CONTEXT D-28.
//
// Visibility: parent owns the `visible` boolean and toggles it on/off as the
// `enhance_progress` WS event stream advances (sending → streaming → done).
// `chars` is shown when defined and > 0; it formats with locale separators
// (en-US comma grouping) so 1234 → "1,234".

// Hex literals match the Phase 1 `@theme` tokens (--color-blsoft, --color-blue,
// --color-ink, --color-mut) — duplicated as JS constants so the inline styles
// below stay self-documenting. The actual blue is consumed via CSS (the
// `.enhancing-dot` and `.enhancing-bar-fill` rules in index.css), so no JS
// constant for it is needed here.
const LILAC = "#ECE9FB"; // --color-blsoft
const TRACK = "#D9D4F4"; // soft lilac track for the animated bar (CONTEXT D-28)
const INK = "#211D18"; // --color-ink
const MUT = "#8A8174"; // --color-mut

export interface EnhancingBannerProps {
  /** When false, the banner is unmounted (no empty container in the DOM). */
  visible: boolean;
  /** Optional running character count from `enhance_progress.chars`. */
  chars?: number;
  /**
   * Copy can be overridden in tests; default matches PRD §5.11 verbatim
   * including the unicode ellipsis (the user-prompt spec calls this out
   * as the load-bearing string).
   */
  copy?: string;
}

const DEFAULT_COPY = "Weaving your notes into the transcript…";

export function EnhancingBanner({
  visible,
  chars,
  copy = DEFAULT_COPY,
}: EnhancingBannerProps) {
  if (!visible) {
    // Render an empty fragment so tests asserting "no children" pass and
    // so the layout doesn't reserve any vertical space.
    return null;
  }

  const hasChars = typeof chars === "number" && chars >= 0;

  return (
    <div
      role="status"
      aria-live="polite"
      data-testid="enhancing-banner"
      style={{
        position: "sticky",
        top: 0,
        zIndex: 20,
        display: "flex",
        alignItems: "center",
        gap: 14,
        padding: "10px 18px",
        backgroundColor: LILAC,
        borderBottom: `1px solid ${TRACK}`,
        color: INK,
        fontSize: 13,
        fontWeight: 600,
      }}
    >
      {/* Pulsing dot — 1.4s recpulse per CONTEXT D-28 / PRD §16.5. */}
      <span className="enhancing-dot" aria-hidden="true" />

      <span style={{ flex: "0 1 auto" }}>{copy}</span>

      {/* Progress track + animated fill — the bar's job is "the AI is
        working" rather than a literal percentage. 1.8s enhancing-bar
        keyframes (defined below in index.css). */}
      <div
        aria-hidden="true"
        style={{
          flex: "1 1 220px",
          height: 4,
          minWidth: 120,
          maxWidth: 320,
          backgroundColor: TRACK,
          borderRadius: 999,
          overflow: "hidden",
        }}
      >
        <div className="enhancing-bar-fill" />
      </div>

      {/* Character count — JetBrains Mono, --color-mut. Only rendered when
        a streaming event has supplied a numeric count. */}
      {hasChars && (
        <span
          data-testid="enhancing-char-count"
          style={{
            fontFamily:
              "JetBrains Mono, ui-monospace, SFMono-Regular, Menlo, monospace",
            fontSize: 12,
            fontWeight: 500,
            color: MUT,
            whiteSpace: "nowrap",
          }}
        >
          {chars!.toLocaleString("en-US")} chars
        </span>
      )}
    </div>
  );
}
