import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { ProviderView } from "../../lib/api/settings";
import { settingsApi } from "../../lib/api/settings";
import { ApiKeyInput } from "./ApiKeyInput";
import { ModelSelect } from "./ModelSelect";

/**
 * Active provider card — Phase 5 (Plan 05-03), SET-04.
 *
 * The 1.5px blueberry border is the **load-bearing visual signature** that
 * "this is the live LLM" (UI-SPEC §Visuals + §Accent reservations #1).
 * Half-step off 1px / 2px is intentional — do not round.
 *
 * The masked-key UX: when a key is stored, the field shows the canonical
 * `••••XXXX` form (server-derived from the last 4 chars) followed by a
 * green matcha `✓ stored` badge. The raw key NEVER lives in React state
 * after `setProviderKey` resolves — see `ApiKeyInput`, which clears its
 * own draft on success so the masked view re-renders from the invalidated
 * query rather than from React state.
 */

interface Props {
  provider: ProviderView;
}

export function ProviderCard({
  provider,
  presetModels = [],
  docsUrl,
  presetName,
}: Props & {
  presetModels?: string[];
  docsUrl?: string;
  /**
   * Brand name from the matching built-in preset (e.g. "Google Gemini").
   * Used as the visible text of the docs link — the user-chosen
   * `provider.name` ("My workspace") would read badly there.
   */
  presetName?: string;
}) {
  const qc = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState({
    name: provider.name,
    base_url: provider.base_url,
    model: provider.model,
  });
  // Draft key lifted out of `ApiKeyInput` so the MODEL `Refresh` button
  // can probe `/v1/models` with it BEFORE the user clicks `Save key`.
  // See the matching comment in `ProviderRow` for the rationale.
  const [apiKeyDraft, setApiKeyDraft] = useState("");

  const update = useMutation({
    mutationFn: () => settingsApi.updateProvider(provider.id, draft),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["settings"] });
      setEditing(false);
    },
  });
  return (
    <article
      className="rounded-xl border-[1.5px] border-[var(--color-blue)] bg-white p-5 shadow-[0_4px_14px_-6px_rgba(91,79,199,0.35)] space-y-4"
      data-testid="active-provider-card"
    >
      <header className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h3 className="font-serif text-[20px] leading-tight">
            {provider.name}
          </h3>
          <span className="text-[10px] font-mono uppercase tracking-[0.06em] bg-[var(--color-blsoft)] text-[var(--color-blue)] px-2 py-0.5 rounded">
            Active
          </span>
        </div>
        <button
          type="button"
          className="text-[12.5px] font-semibold text-mut hover:text-ink"
          onClick={() => setEditing((e) => !e)}
        >
          {editing ? "Cancel" : "Edit"}
        </button>
      </header>

      <div className="grid grid-cols-2 gap-x-6 gap-y-3">
        <Field label="BASE URL">
          {editing ? (
            <input
              className="w-full font-mono text-[12.5px] border-b border-line focus:border-[var(--color-blue)] outline-none py-1"
              value={draft.base_url}
              onChange={(e) =>
                setDraft({ ...draft, base_url: e.target.value })
              }
            />
          ) : (
            <code className="font-mono text-[12.5px] text-ink">
              {provider.base_url}
            </code>
          )}
        </Field>
        <Field label="MODEL">
          {editing ? (
            <ModelSelect
              providerId={provider.id}
              providerName={provider.name}
              value={draft.model}
              onChange={(next) => setDraft({ ...draft, model: next })}
              presetModels={presetModels}
              hasStoredKey={!!provider.api_key_masked}
              apiKeyDraft={apiKeyDraft}
            />
          ) : (
            <code className="font-mono text-[12.5px] text-ink">
              {provider.model || "—"}
            </code>
          )}
        </Field>
      </div>
      {docsUrl && (
        <a
          href={docsUrl}
          target="_blank"
          rel="noopener noreferrer"
          className="text-[11px] text-[var(--color-blue)] hover:underline inline-block"
        >
          See all {presetName ?? provider.name} models →
        </a>
      )}

      {editing ? (
        <button
          type="button"
          className="text-sm bg-[var(--color-blue)] text-white px-3 py-1.5 rounded-md disabled:opacity-50"
          disabled={update.isPending}
          onClick={() => update.mutate()}
        >
          {update.isPending ? "Saving…" : "Save"}
        </button>
      ) : null}

      <div className="border-t border-line pt-3 space-y-2">
        <div className="text-[10px] font-mono uppercase tracking-[0.06em] text-grey">
          API KEY · in Keychain
        </div>
        {provider.api_key_masked ? (
          <div className="flex items-center gap-2 text-[12.5px] font-mono">
            <span className="text-ink">{provider.api_key_masked}</span>
            <span className="text-[var(--color-matcha)] font-semibold">
              ✓ stored
            </span>
          </div>
        ) : (
          <div className="text-sm text-mut">No key stored yet.</div>
        )}
        <div className="pt-1">
          <ApiKeyInput
            providerId={provider.id}
            providerName={provider.name}
            hasStoredKey={!!provider.api_key_masked}
            onDraftChange={setApiKeyDraft}
          />
        </div>
      </div>
    </article>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="space-y-1">
      <div className="text-[10px] font-mono uppercase tracking-[0.06em] text-grey">
        {label}
      </div>
      <div>{children}</div>
    </div>
  );
}
