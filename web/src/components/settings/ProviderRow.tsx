import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { ProviderView } from "../../lib/api/settings";
import { settingsApi } from "../../lib/api/settings";
import { ApiKeyInput } from "./ApiKeyInput";
import { ModelSelect } from "./ModelSelect";
import { TestKeyButton } from "./TestKeyButton";

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
 * the card stays compact until the user opts into keying — but `Test`
 * sits next to `Replace key` in the collapsed state, because a provider
 * that is already keyed is the one most worth probing.
 *
 * `autoOpenKey` opens the key input on first mount and seeds its
 * `autoFocus`, so cloning a preset chip lands the user at the key field
 * with the cursor already there — the obvious next action.
 */

export function ProviderRow({
  provider,
  autoOpenKey = false,
  presetModels = [],
  docsUrl,
  presetName,
  defaultCliModel = "",
  onKeyClosed,
}: {
  provider: ProviderView;
  autoOpenKey?: boolean;
  presetModels?: string[];
  docsUrl?: string;
  /** Brand name from the matching built-in preset (e.g. "Google Gemini").
   *  Used as the visible text of the docs link — the user-chosen
   *  `provider.name` would read badly there. */
  presetName?: string;
  /** `cli`-adapter only: the preset's suggested `--model` (e.g. "haiku"),
   *  seeded into `cli_model` the first time a Test proves the CLI
   *  connects. Empty for a preset with no sensible default. */
  defaultCliModel?: string;
  /** Called whenever the API key section collapses (Save or Cancel), so
   *  the page can drop its one-shot auto-open hint (`autoOpenKey`) for
   *  this provider. */
  onKeyClosed?: () => void;
}) {
  const isCli = provider.adapter === "cli";
  const qc = useQueryClient();
  // A `cli` provider has no key section to auto-open - `autoOpenKey` only
  // ever applies to a freshly-cloned `http` provider.
  const [keying, setKeying] = useState(autoOpenKey && !isCli);
  const [modelDraft, setModelDraft] = useState(provider.model);
  const [cliModelDraft, setCliModelDraft] = useState(provider.cli_model);
  // The MODEL picker is gated behind proof the CLI actually connects - a
  // freshly-cloned row has no `cli_model` yet, and it stays hidden until a
  // Test comes back `ok`. `!!provider.cli_model` alone would cover a row
  // that already picked a model in an earlier session; the local flag
  // covers the same session's just-passed Test before that PATCH lands.
  const [cliTestedOk, setCliTestedOk] = useState(false);
  const cliVerified = !!provider.cli_model || cliTestedOk;
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
  useEffect(() => {
    setCliModelDraft(provider.cli_model);
  }, [provider.cli_model]);
  const activate = useMutation({
    mutationFn: () => settingsApi.activateProvider(provider.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  const remove = useMutation({
    mutationFn: () => settingsApi.deleteProvider(provider.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  // PATCH on commit - a pick from the dropdown, pressing Enter, or blurring
  // the field - never per keystroke, so free typing doesn't fire a PATCH
  // per character. `provider.model` is the source of truth; we only PATCH
  // when the committed value actually differs from it.
  const updateModel = useMutation({
    mutationFn: (next: string) =>
      settingsApi.updateProvider(provider.id, {
        name: provider.name,
        base_url: provider.base_url,
        model: next,
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  // Same PATCH-on-commit shape as `updateModel`, but for the CLI's
  // `--model` override. `model` is sent back unchanged - it holds the
  // CLI program id ("claude"/"cursor-agent"), never user-edited.
  const updateCliModel = useMutation({
    mutationFn: (next: string) =>
      settingsApi.updateProvider(provider.id, {
        name: provider.name,
        base_url: provider.base_url,
        model: provider.model,
        cli_model: next,
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });

  return (
    <article
      className="rounded-xl border border-line bg-white p-5 space-y-4"
      data-testid="inactive-provider-card"
    >
      <header className="flex items-center justify-between">
        <h3 className="heading-card">
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

      {isCli ? (
        <div className="space-y-3">
          <div className="rounded-lg bg-[var(--color-paper)] px-3 py-2 text-[12.5px] text-mut">
            Runs <code className="font-mono text-ink">{provider.model}</code>{" "}
            locally via your existing CLI login. No API key or base URL to
            configure.
          </div>
          {cliVerified ? (
            <Field label="MODEL">
              <ModelSelect
                providerId={provider.id}
                providerName={provider.name}
                value={cliModelDraft}
                onChange={setCliModelDraft}
                onCommit={(next) => {
                  if (next !== provider.cli_model) updateCliModel.mutate(next);
                }}
                presetModels={presetModels}
                placeholder="CLI default"
              />
              {updateCliModel.isError && (
                <p role="status" className="text-[11px] text-[var(--color-straw)]">
                  ✗ Could not save model: {String(updateCliModel.error)}
                </p>
              )}
            </Field>
          ) : (
            <p className="text-[11px] text-mut">
              Click Test below to confirm the CLI connects - the model
              picker appears once it does.
            </p>
          )}
        </div>
      ) : (
        <>
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
                onChange={setModelDraft}
                onCommit={(next) => {
                  if (next !== provider.model) updateModel.mutate(next);
                }}
                presetModels={presetModels}
                apiKeyDraft={apiKeyDraft}
              />
              {updateModel.isError && (
                <p role="status" className="text-[11px] text-[var(--color-straw)]">
                  ✗ Could not save model: {String(updateModel.error)}
                </p>
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
        </>
      )}

      <div className="border-t border-line pt-3 space-y-2">
        {isCli ? (
          // No key concept at all - `Test` just confirms the CLI resolves
          // and answers.
          <div className="flex flex-wrap items-center gap-3">
            <TestKeyButton
              providerId={provider.id}
              providerName={provider.name}
              alwaysTestable
              onResult={(result) => {
                if (!result.ok) return;
                setCliTestedOk(true);
                if (!provider.cli_model && defaultCliModel) {
                  updateCliModel.mutate(defaultCliModel);
                }
              }}
            />
          </div>
        ) : (
          <>
            <div className="text-[10px] font-mono uppercase tracking-[0.06em] text-grey">
              API KEY · stored locally
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
                  onSaved={() => {
                    setKeying(false);
                    onKeyClosed?.();
                  }}
                  onDraftChange={setApiKeyDraft}
                />
                <button
                  type="button"
                  onClick={() => {
                    setKeying(false);
                    setApiKeyDraft("");
                    onKeyClosed?.();
                  }}
                  className="text-[12.5px] font-semibold text-mut hover:text-ink"
                >
                  Cancel
                </button>
              </div>
            ) : (
              // Collapsed: `Test` stays reachable so a provider that is fully
              // set up can be probed without pretending to replace its key.
              <div className="flex flex-wrap items-center gap-3 pt-1">
                <button
                  type="button"
                  onClick={() => setKeying(true)}
                  className="text-[12.5px] font-semibold text-mut hover:text-ink"
                >
                  {provider.api_key_masked ? "Replace key" : "Add key"}
                </button>
                <TestKeyButton
                  providerId={provider.id}
                  providerName={provider.name}
                  hasStoredKey={!!provider.api_key_masked}
                />
              </div>
            )}
          </>
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
    <div className="space-y-1 min-w-0">
      <div className="text-[10px] font-mono uppercase tracking-[0.06em] text-grey">
        {label}
      </div>
      <div>{children}</div>
    </div>
  );
}
