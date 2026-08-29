import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { ProviderView } from "../../lib/api/settings";
import { settingsApi } from "../../lib/api/settings";
import { ApiKeyInput } from "./ApiKeyInput";
import { ModelSelect } from "./ModelSelect";

/**
 * Inactive provider card — mirrors `ProviderCard`'s structure so every
 * provider (active or not) reads as a card. The 1.5px blueberry border,
 * ACTIVE badge, soft shadow, and Edit toggle are reserved for the active
 * card (UI-SPEC §Visuals + §Accent reservations #1) — the inactive card
 * uses a regular `--color-line` border, no shadow, and exposes
 * `Set active` as the primary footer action instead of Edit.
 *
 * BASE URL stays read-only here — it's a structural field the user almost
 * never wants to change after the provider exists. MODEL is editable via
 * the `ModelSelect` dropdown (preset hints + live `/v1/models` after a
 * key is stored) and persists on selection, so the user can switch from
 * `gemini-2.5-flash` to `gemini-2.5-pro` before activating without first
 * promoting the provider to active. The API KEY section is collapsible so
 * the card stays compact until the user opts into keying.
 *
 * `autoOpenKey` opens the key input on first mount and seeds its
 * `autoFocus`, so cloning a preset chip lands the user at the key field
 * with the cursor already there — the obvious next action.
 */

export function ProviderRow({
  provider,
  autoOpenKey = false,
  presetModels = [],
}: {
  provider: ProviderView;
  autoOpenKey?: boolean;
  presetModels?: string[];
}) {
  const qc = useQueryClient();
  const [keying, setKeying] = useState(autoOpenKey);
  const [modelDraft, setModelDraft] = useState(provider.model);
  // Draft key lifted out of `ApiKeyInput` so the MODEL `Refresh` button
  // can probe `/v1/models` with it BEFORE the user clicks `Save key`.
  // The whole point: if the saved `model` is the only thing wrong with
  // the provider (Google's `gemini-2.5-flash` deprecation is the
  // canonical case), the user needs to see what IS available before
  // they can pick a replacement.
  const [apiKeyDraft, setApiKeyDraft] = useState("");
  // Sync the local draft when the cached provider model changes (after a
  // successful PATCH invalidates the query, or when this row is reused for
  // a different provider by some future code path).
  useEffect(() => {
    setModelDraft(provider.model);
  }, [provider.model]);
  const activate = useMutation({
    mutationFn: () => settingsApi.activateProvider(provider.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  const remove = useMutation({
    mutationFn: () => settingsApi.deleteProvider(provider.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  // PATCH on selection — every preset option is a discrete change, so a
  // debounce would just hide the fact that this fires. `provider.model` is
  // the source of truth; we only PATCH when the draft actually differs.
  const updateModel = useMutation({
    mutationFn: (next: string) =>
      settingsApi.updateProvider(provider.id, {
        name: provider.name,
        base_url: provider.base_url,
        model: next,
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });

  return (
    <article
      className="rounded-xl border border-line bg-white p-5 space-y-4"
      data-testid="inactive-provider-card"
    >
      <header className="flex items-center justify-between">
        <h3 className="font-serif text-[20px] leading-tight">
          {provider.name}
        </h3>
        <button
          type="button"
          className="text-[12.5px] font-semibold text-mut hover:text-[var(--color-straw)] disabled:opacity-50"
          onClick={() => remove.mutate()}
          disabled={remove.isPending}
          aria-label={`Remove provider ${provider.name}`}
        >
          Remove
        </button>
      </header>

      <div className="grid grid-cols-2 gap-x-6 gap-y-3">
        <Field label="BASE URL">
          <code className="font-mono text-[12.5px] text-ink break-all">
            {provider.base_url}
          </code>
        </Field>
        <Field label="MODEL">
          <ModelSelect
            providerId={provider.id}
            providerName={provider.name}
            value={modelDraft}
            onChange={(next) => {
              setModelDraft(next);
              if (next !== provider.model) updateModel.mutate(next);
            }}
            presetModels={presetModels}
            hasStoredKey={!!provider.api_key_masked}
            apiKeyDraft={apiKeyDraft}
          />
          {updateModel.isError && (
            <p role="status" className="text-[11px] text-[var(--color-straw)]">
              ✗ Could not save model: {String(updateModel.error)}
            </p>
          )}
        </Field>
      </div>

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
        {keying ? (
          <div className="pt-1 space-y-2">
            <ApiKeyInput
              providerId={provider.id}
              providerName={provider.name}
              hasStoredKey={!!provider.api_key_masked}
              autoFocus
              onSaved={() => setKeying(false)}
              onDraftChange={setApiKeyDraft}
            />
            <button
              type="button"
              onClick={() => setKeying(false)}
              className="text-[12.5px] font-semibold text-mut hover:text-ink"
            >
              Cancel
            </button>
          </div>
        ) : (
          <button
            type="button"
            onClick={() => setKeying(true)}
            className="text-[12.5px] font-semibold text-mut hover:text-ink"
          >
            {provider.api_key_masked ? "Replace key" : "Add key"}
          </button>
        )}
      </div>

      <div className="flex justify-end">
        <button
          type="button"
          className="text-sm font-semibold bg-[var(--color-blue)] text-white px-3 py-1.5 rounded-md disabled:opacity-50"
          onClick={() => activate.mutate()}
          disabled={activate.isPending}
        >
          {activate.isPending ? "Activating…" : "Set active"}
        </button>
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
