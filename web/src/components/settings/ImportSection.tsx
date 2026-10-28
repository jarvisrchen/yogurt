import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router";
import { Button } from "../Button";
import { Card } from "../Card";
import {
  IMPORT_KEY,
  importApi,
  type ImportCounts,
  type ImportPreview,
} from "../../lib/api/granolaImport";
import {
  applyImportStatus,
  clearImport,
  markImportStarted,
  useGranolaImport,
  type ImportState,
} from "../../hooks/useGranolaImport";
import { INPUT_MONO } from "./cloud/Field";
import { LABEL } from "./labelClass";
import { Note } from "./cloud/Note";

const HELP_URL = "https://docs.granola.ai/help-center/sharing/integrations/granola-api";
const n = (v: number) => v.toLocaleString("en-US");
const meetings = (v: number) => `${n(v)} ${v === 1 ? "meeting" : "meetings"}`;

function Header() {
  return (
    <header>
      <h2 className="heading-section">Import</h2>
      <p className="mb-4 mt-1 text-[13px] text-ai">
        Copy your meetings, notes, and transcripts from Granola into yogurt. The import reads from Granola and never changes your Granola account.
      </p>
    </header>
  );
}

function KeyForm({ onSaved }: { onSaved: () => void }) {
  const qc = useQueryClient();
  const [draft, setDraft] = useState("");
  const save = useMutation({
    mutationFn: importApi.setKey,
    gcTime: 0,
    onSuccess: () => {
      setDraft("");
      qc.invalidateQueries({ queryKey: IMPORT_KEY });
      onSaved();
    },
  });
  return (
    <form
      className="space-y-2"
      onSubmit={(e) => {
        e.preventDefault();
        if (draft.trim()) save.mutate(draft);
      }}
    >
      <div className={LABEL}>Granola API key · stored locally</div>
      <div className="flex flex-wrap items-center gap-2">
        <input
          type="password"
          autoComplete="off"
          placeholder="Paste key…"
          aria-label="Granola API key"
          className={`${INPUT_MONO} min-w-0 flex-1`}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <Button type="submit" variant="primary" size="sm" disabled={!draft.trim() || save.isPending}>
          {save.isPending ? "Checking…" : "Save key"}
        </Button>
      </div>
      {save.isError && (
        <Note tone="straw" role="alert">
          {save.error.message}
        </Note>
      )}
    </form>
  );
}

function NoKey() {
  return (
    <Card padding="compact" className="space-y-3">
      <p className="text-[13px] text-ink">
        In the Granola desktop app, open Settings, Connectors, API keys, and create a key with access to your notes.{" "}
        <a href={HELP_URL} target="_blank" rel="noopener noreferrer" className="font-medium text-blue hover:underline">
          Granola API help ↗
        </a>
      </p>
      <p className="text-[13px] text-ink">
        Paste the key here. It is checked with Granola before it is saved, and kept in <code className="font-mono">keys.json</code> on this Mac.
      </p>
      <KeyForm onSaved={() => {}} />
    </Card>
  );
}

function Stat({ value, label }: { value: number; label: string }) {
  return (
    <div className="flex flex-col">
      <b className="text-[18px] font-extrabold">{n(value)}</b>
      <span className="text-[11px] text-mut">{label}</span>
    </div>
  );
}

function KeySaved({ masked, cancelled }: { masked: string; cancelled?: Extract<ImportState, { kind: "cancelled" }> }) {
  const qc = useQueryClient();
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const remove = useMutation({
    mutationFn: importApi.deleteKey,
    onSuccess: () => {
      setPreview(null);
      clearImport();
      qc.invalidateQueries({ queryKey: IMPORT_KEY });
    },
  });
  const run = useMutation({
    mutationFn: importApi.preview,
    onMutate: () => clearImport(),
    onSuccess: setPreview,
  });
  const start = useMutation({
    mutationFn: importApi.start,
    onSuccess: markImportStarted,
  });
  const error = remove.error ?? run.error ?? start.error;

  return (
    <Card padding="compact" className="space-y-4">
      <div className="space-y-1">
        <div className={LABEL}>Granola API key · stored locally</div>
        <div className="flex items-center gap-3">
          <span className="font-mono text-[13px] text-ink">{masked}</span>
          <span className="text-[12.5px] font-medium text-matcha">✓ stored</span>
          <Button variant="ghost" size="sm" disabled={remove.isPending} onClick={() => remove.mutate()}>
            Remove
          </Button>
        </div>
      </div>

      {cancelled && (
        <Note tone="honey" role="status">
          Import cancelled. {meetings(cancelled.imported)} imported before it stopped. A new import picks up where this one left off.
        </Note>
      )}

      <div className="border-t border-line pt-4">
        {preview ? (
          <div className="space-y-3">
            <p className="text-[13px] text-ink" data-testid="import-preview">
              {meetings(preview.total)} in Granola · {n(preview.already_imported)} already imported · {n(preview.captured_by_others)} captured by someone else
            </p>
            <div className="flex items-center gap-2">
              <Button variant="primary" size="sm" disabled={preview.to_import === 0 || start.isPending} onClick={() => start.mutate()}>
                {preview.to_import === 0 ? "Nothing new to import" : `Import ${meetings(preview.to_import)}`}
              </Button>
              <Button variant="secondary" size="sm" disabled={run.isPending} onClick={() => run.mutate()}>
                Refresh
              </Button>
            </div>
          </div>
        ) : (
          <Button variant="secondary" size="sm" disabled={run.isPending} onClick={() => run.mutate()}>
            {run.isPending ? "Counting…" : "Preview"}
          </Button>
        )}
      </div>

      {error && (
        <Note tone="straw" role="alert">
          {error.message}
        </Note>
      )}
    </Card>
  );
}

function Running({ progress }: { progress: ImportCounts }) {
  const cancel = useMutation({ mutationFn: importApi.cancel });
  const pct = progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0;
  return (
    <Card padding="compact" className="space-y-4">
      <div className="flex items-baseline justify-between gap-3">
        <div className="text-[13px] font-medium text-ink">
          {progress.total > 0 ? `${n(progress.done)} of ${n(progress.total)}` : "Listing your Granola notes…"}
        </div>
        <Button variant="secondary" size="sm" disabled={cancel.isPending} onClick={() => cancel.mutate()}>
          {cancel.isPending ? "Cancelling…" : "Cancel"}
        </Button>
      </div>
      <div
        role="progressbar"
        aria-label="Import progress"
        aria-valuemin={0}
        aria-valuemax={progress.total}
        aria-valuenow={progress.done}
        className="h-2 overflow-hidden rounded-pill bg-line"
      >
        <div className="h-full rounded-pill bg-matcha transition-[width] duration-200" style={{ width: `${pct}%` }} />
      </div>
      <div className="flex gap-8">
        <Stat value={progress.imported} label="imported" />
        <Stat value={progress.skipped} label="skipped" />
        <Stat value={progress.failed} label="failed" />
      </div>
      {cancel.isError && (
        <Note tone="straw" role="alert">
          {cancel.error.message}
        </Note>
      )}
    </Card>
  );
}

function Done({ result }: { result: Extract<ImportState, { kind: "done" }> }) {
  const more = result.failed - result.failedTitles.length;
  return (
    <Card padding="compact" className="space-y-4">
      <div className="text-[13px] font-medium text-ink">Import finished</div>
      <div className="flex gap-8">
        <Stat value={result.imported} label="imported" />
        <Stat value={result.skipped} label="skipped" />
        <Stat value={result.failed} label="failed" />
      </div>
      {result.failed > 0 && (
        <div className="space-y-1">
          <div className={LABEL}>Could not import</div>
          <ul className="list-disc pl-4 text-[12.5px] text-ai">
            {result.failedTitles.map((t, i) => (
              <li key={i}>{t}</li>
            ))}
          </ul>
          {more > 0 && <p className="text-[12px] text-mut">and {n(more)} more</p>}
          <p className="text-[12px] text-mut">Run the import again to retry these.</p>
        </div>
      )}
      <div className="flex items-center gap-4">
        <Link to="/" className="text-[13px] font-medium text-blue hover:underline">
          Open the Library
        </Link>
        <Button variant="ghost" size="sm" onClick={clearImport}>
          Close
        </Button>
      </div>
    </Card>
  );
}

function Failed({ error }: { error: string }) {
  return (
    <Card padding="compact" className="space-y-3">
      <Note tone="straw" role="alert">
        <b>Import failed.</b> {error}
      </Note>
      <p className="text-[12.5px] text-mut">Meetings imported before the failure stay in the Library. A new import resumes after them.</p>
      <Button variant="secondary" size="sm" onClick={clearImport}>
        Back
      </Button>
    </Card>
  );
}

export function ImportSection() {
  const status = useQuery({ queryKey: IMPORT_KEY, queryFn: importApi.status, retry: false });
  const imp = useGranolaImport();

  // Cached data can predate a finished run, so only a settled fetch re-syncs the shared store.
  useEffect(() => {
    if (status.data && !status.isFetching) applyImportStatus(status.data);
  }, [status.data, status.isFetching]);

  const state = imp?.state;
  let body;
  if (state?.kind === "running") body = <Running progress={state.progress} />;
  else if (state?.kind === "done") body = <Done result={state} />;
  else if (state?.kind === "failed") body = <Failed error={state.error} />;
  else if (status.isError) body = <Note tone="straw" role="alert">Failed to load: {status.error.message}</Note>;
  else if (!status.data) body = <div className="h-24 rounded-card shimmer" aria-busy="true" />;
  else if (!status.data.key) body = <NoKey />;
  else body = <KeySaved masked={status.data.key} cancelled={state?.kind === "cancelled" ? state : undefined} />;

  return (
    <section>
      <Header />
      {body}
    </section>
  );
}
