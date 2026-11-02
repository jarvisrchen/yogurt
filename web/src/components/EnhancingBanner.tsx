const LILAC = "var(--color-blsoft)";
// Lilac track mixed from the accent so it works on light and dark surfaces.
const TRACK = "color-mix(in srgb, var(--color-blue) 22%, transparent)";
const INK = "var(--color-ink)";
const MUT = "var(--color-mut)";
const STRAW = "var(--color-straw)";
const STRAW_SOFT = "var(--color-strsoft)";

export interface EnhancingBannerProps {
  visible: boolean;
  chars?: number;
  copy?: string;
  /** If non-null, shows a static error state with a Retry button (when `onRetry` is set). */
  errorMessage?: string | null;
  onRetry?: () => void;
  /** Puts the bar on its own row, for a phone-width screen the one row does not fit. */
  stacked?: boolean;
}

const DEFAULT_COPY = "Weaving your notes into the transcript…";

export function EnhancingBanner({
  visible,
  chars,
  copy = DEFAULT_COPY,
  errorMessage,
  onRetry,
  stacked = false,
}: EnhancingBannerProps) {
  // The error state wins over `visible`, so the parent can leave `visible` true until retry.
  if (errorMessage) {
    return (
      <div
        role="alert"
        data-testid="enhancing-banner-error"
        style={{
          position: "sticky",
          top: 0,
          zIndex: 20,
          display: "flex",
          alignItems: "center",
          gap: 14,
          padding: "10px 18px",
          backgroundColor: STRAW_SOFT,
          borderBottom: `1px solid ${STRAW}`,
          color: STRAW,
          fontSize: 13,
          fontWeight: 600,
        }}
      >
        <span aria-hidden="true">⚠</span>
        <span style={{ flex: "1 1 auto" }}>{errorMessage}</span>
        {onRetry && (
          <button
            type="button"
            data-testid="enhancing-banner-retry"
            onClick={onRetry}
            style={{
              padding: "6px 12px",
              borderRadius: 8,
              backgroundColor: STRAW,
              color: "#FFFFFF",
              fontSize: 12,
              fontWeight: 600,
              border: "none",
              cursor: "pointer",
            }}
          >
            Retry
          </button>
        )}
      </div>
    );
  }

  if (!visible) {
    // Empty fragment: reserves no vertical space.
    return null;
  }

  const hasChars = typeof chars === "number" && chars >= 0;
  const dot = <span className="enhancing-dot" aria-hidden="true" />;
  const bar = (
    <div
      aria-hidden="true"
      style={{
        ...(stacked ? {} : { flex: "1 1 220px", minWidth: 120, maxWidth: 320 }),
        height: 4,
        backgroundColor: TRACK,
        borderRadius: 999,
        overflow: "hidden",
      }}
    >
      <div className="enhancing-bar-fill" />
    </div>
  );
  const count = hasChars && (
    <span
      data-testid="enhancing-char-count"
      className="font-mono"
      style={{
        fontSize: 12,
        fontWeight: 500,
        color: MUT,
        whiteSpace: "nowrap",
      }}
    >
      {chars!.toLocaleString("en-US")} chars
    </span>
  );

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
        ...(stacked ? { flexDirection: "column", gap: 8, padding: "10px 16px" } : { alignItems: "center", gap: 14, padding: "10px 18px" }),
        backgroundColor: LILAC,
        borderBottom: `1px solid ${TRACK}`,
        color: INK,
        fontSize: 13,
        fontWeight: 600,
      }}
    >
      {stacked ? (
        <>
          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            {dot}
            <span style={{ flex: "1 1 auto" }}>{copy}</span>
            {count}
          </div>
          {bar}
        </>
      ) : (
        <>
          {dot}
          <span style={{ flex: "0 1 auto" }}>{copy}</span>
          {bar}
          {count}
        </>
      )}
    </div>
  );
}
