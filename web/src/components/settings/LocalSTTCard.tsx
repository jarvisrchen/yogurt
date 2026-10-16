// `activeDownload` is the truth about an in-flight download and survives the
// dialog closing. `dialogOpen` only tracks modal visibility.
// The WS hook lives here, not in the dialog, so progress persists across open/close.
// Only one download is tracked in the UI. A second pill click replaces it,
// though the server still finishes the first.
import { useEffect, useState } from "react";
import clsx from "clsx";
import { useModels, useDeleteModel, formatBytes } from "../../lib/api/stt";
import { settingsApi } from "../../lib/api/settings";
import { useModelDownloadProgress } from "../../hooks/useModelDownloadProgress";
import { ModelPicker } from "./ModelPicker";
import { ModelDownloadDialog } from "../dialogs/ModelDownloadDialog";
import { TestKeyButton } from "./TestKeyButton";
import { Button } from "../Button";
import { Pill } from "../Pill";

// http() throws "<status> <statusText>: <body>". The 409 body is a plain sentence, so strip the prefix.
function deleteErrorMessage(err: unknown): string {
  const raw = err instanceof Error ? err.message : String(err);
  // `[^:]*` not `+`: HTTP/2 has no reason phrase, so the prefix can be "409 : ".
  return raw.replace(/^\d+\s+[^:]*:\s*/, "");
}

interface LocalSTTCardProps {
  active: boolean;
  selectedModel: string;
  onSelectModel: (name: string) => void;
  onActivate: () => void;
}

export function LocalSTTCard({
  active,
  selectedModel,
  onSelectModel,
  onActivate,
}: LocalSTTCardProps) {
  const q = useModels();
  const del = useDeleteModel();
  // Tracked apart from `del.isSuccess` so a later error leaves no stale "Deleted" line.
  const [showFreed, setShowFreed] = useState(false);
  useEffect(() => {
    if (!del.isSuccess) return;
    setShowFreed(true);
    const t = window.setTimeout(() => setShowFreed(false), 4000);
    return () => window.clearTimeout(t);
  }, [del.isSuccess, del.data]);
  const [activeDownload, setActiveDownload] = useState<{
    name: string;
    sizeMb: number;
  } | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);

  const progress = useModelDownloadProgress(activeDownload?.name ?? null);

  // Clear 600 ms after success so 100% shows briefly. Never auto-clear on error:
  // the pill stays reopenable so the user can read it and retry (resumes the partial file).
  useEffect(() => {
    if (!progress?.complete) return;
    const t = window.setTimeout(() => setActiveDownload(null), 600);
    return () => window.clearTimeout(t);
  }, [progress?.complete]);

  const startOrReopenDownload = (name: string) => {
    const m = q.data?.find((x) => x.name === name);
    if (!m) return;
    if (activeDownload?.name === name) {
      setDialogOpen(true);
      return;
    }
    setActiveDownload({ name: m.name, sizeMb: m.size_mb });
    setDialogOpen(true);
  };

  // The selected model must be on disk or recording fails at meeting start.
  // Deliberately not gated on `active`, so a provider already set to local
  // with a deleted model still shows the warning.
  const selectedModelDownloaded =
    q.data?.find((m) => m.name === selectedModel)?.downloaded ?? false;
  const activateBlocked = !selectedModelDownloaded;

  const handleDialogClose = () => {
    setDialogOpen(false);
    // Closing on an error dismisses the failed download. A running download stays tracked.
    if (progress?.error) {
      setActiveDownload(null);
    }
  };

  return (
    <article
      data-testid="local-stt-card"
      className={clsx(
        "rounded-card bg-card p-5 space-y-4 transition-colors",
        active
          ? "border-[1.5px] border-blue shadow-button-blue"
          : "border border-line",
      )}
    >
      <header className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2 min-w-0">
          <h3 className="heading-card whitespace-nowrap">Local</h3>
          <Pill tone="neutral">whisper.cpp</Pill>
        </div>
        {active ? (
          <Pill tone="matcha" className="shrink-0">
            <span
              className="inline-block w-[7px] h-[7px] rounded-pill bg-matcha"
              aria-hidden
            />
            In use
          </Pill>
        ) : (
          <Button
            variant="primary"
            className="px-3 py-1.5 text-[13px] whitespace-nowrap shrink-0"
            disabled={activateBlocked}
            // Guard in the handler too so a synthetic click on a disabled button cannot activate.
            onClick={() => {
              if (!activateBlocked) onActivate();
            }}
          >
            Use Local
          </Button>
        )}
      </header>
      {activateBlocked && (
        <span className="text-[11px] text-straw">
          Download the model first
        </span>
      )}

      <p className="text-[12.5px] text-mut">
        Whisper.cpp with Metal acceleration. Audio never leaves this Mac.
        Local is free.
      </p>

      {q.isLoading && (
        <p className="text-[12.5px] text-mut">Loading models…</p>
      )}
      {q.isError && (
        <p className="text-[12.5px] text-straw">
          Failed to load models: {String(q.error)}
        </p>
      )}
      {q.data && (
        <ModelPicker
          models={q.data}
          selected={selectedModel}
          onSelect={onSelectModel}
          onRequestDownload={startOrReopenDownload}
          activeDownloadName={activeDownload?.name ?? null}
          activeDownloadProgress={progress}
          onDelete={(name) => del.mutate(name)}
          activeModelName={active ? selectedModel : null}
        />
      )}

      {q.data && (
        <div className="flex flex-wrap items-center gap-2 border-t border-line pt-4 mt-4">
          <TestKeyButton
            providerName={selectedModel}
            hasStoredKey={selectedModelDownloaded}
            testFn={() => settingsApi.testLocalStt(selectedModel)}
          />
        </div>
      )}

      {del.isError && (
        <p role="alert" className="text-[12.5px] text-straw">
          {deleteErrorMessage(del.error)}
        </p>
      )}
      {showFreed && del.data && (
        <p
          data-testid="model-freed"
          className="text-[12.5px] text-matcha"
        >
          Deleted {del.variables}
          {del.data.freed_bytes > 0
            ? ` - freed ${formatBytes(del.data.freed_bytes)}`
            : ""}
        </p>
      )}

      <p className="text-[12.5px] text-mut pt-1">
        Models download on first use · stored in{" "}
        <code className="font-mono text-ink">~/.yogurt/models/</code>
      </p>

      <ModelDownloadDialog
        model={dialogOpen ? activeDownload?.name ?? null : null}
        sizeMb={activeDownload?.sizeMb ?? null}
        progress={progress}
        onClose={handleDialogClose}
      />
    </article>
  );
}
