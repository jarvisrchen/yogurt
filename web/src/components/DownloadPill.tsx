import clsx from "clsx";
import { useNavigate } from "react-router";
import {
  dismissDownload,
  showDownloadDialog,
  useModelDownload,
} from "../hooks/useModelDownload";
import { ProgressRing } from "./ProgressRing";

export function DownloadPill() {
  const dl = useModelDownload();
  const navigate = useNavigate();
  if (!dl) return null;

  const { model, progress } = dl;
  const pct =
    progress.totalBytes > 0
      ? Math.min(100, Math.round((progress.bytesDownloaded / progress.totalBytes) * 100))
      : null;

  const [label, detail, onClick] = progress.error
    ? [`Show download error for ${model}`, "failed", showDownloadDialog]
    : progress.complete
      ? [
          `${model} downloaded, open transcription settings`,
          "ready",
          () => {
            dismissDownload();
            navigate("/settings/transcription");
          },
        ]
      : [
          `Show download progress for ${model}`,
          pct == null ? "starting…" : `${pct}%`,
          showDownloadDialog,
        ];

  return (
    <button
      type="button"
      onClick={onClick}
      aria-label={label}
      data-testid="download-pill"
      className={clsx(
        "flex items-center gap-2 px-3 py-1.5 bg-card border rounded-full shadow-pop text-[13px] text-ink transition-colors",
        progress.error ? "border-straw" : "border-line hover:border-matcha",
      )}
    >
      {progress.error || progress.complete ? (
        <span
          aria-hidden="true"
          className={clsx(
            "inline-flex w-3.5 justify-center font-semibold",
            progress.error ? "text-straw" : "text-matcha",
          )}
        >
          {progress.error ? "!" : "✓"}
        </span>
      ) : (
        <ProgressRing pct={pct} />
      )}
      <span className="font-medium truncate max-w-[180px]">{model}</span>
      <span
        className={clsx(
          "font-mono text-[12px] tracking-tight",
          progress.error ? "text-straw" : "text-mut",
        )}
      >
        {detail}
      </span>
    </button>
  );
}
