import { useEffect, useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { ProviderView } from "../../lib/api/settings";
import { settingsApi } from "../../lib/api/settings";
import { ApiKeyInput } from "./ApiKeyInput";
import { ModelSelect } from "./ModelSelect";
import { TestKeyButton } from "./TestKeyButton";
import { Button } from "../Button";
import { LABEL } from "./labelClass";

const VALUE = "text-[13.5px] text-ink";

// The inactive card has a plain line border and no shadow. The 1.5px blueberry
// border, ACTIVE badge and Edit toggle are reserved for the active card.
// `Test` sits next to `Replace key` while collapsed so a keyed provider can still be probed.

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
  // Visible text of the docs link. The user-chosen `provider.name` would read badly there.
  presetName?: string;
  // `cli` only: seeded into `cli_model` the first time a Test proves the CLI connects.
  defaultCliModel?: string;
  // Lets the page drop its one-shot `autoOpenKey` hint for this provider.
  onKeyClosed?: () => void;
}) {
  const isCli = provider.adapter === "cli";
  const qc = useQueryClient();
  const [keying, setKeying] = useState(autoOpenKey && !isCli);
  const [modelDraft, setModelDraft] = useState(provider.model);
  const [cliModelDraft, setCliModelDraft] = useState(provider.cli_model);
  // The MODEL picker stays hidden until a Test proves the CLI connects. The local
  // flag covers a Test that passed before the `cli_model` PATCH lands.
  const [cliTestedOk, setCliTestedOk] = useState(false);
  const cliVerified = !!provider.cli_model || cliTestedOk;
  // Lifted out of `ApiKeyInput` so Refresh can list models before the key is saved,
  // which matters when the saved model is the only thing wrong (a deprecated model).
  const [apiKeyDraft, setApiKeyDraft] = useState("");
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
  // PATCH on commit, never per keystroke, and only when the value differs.
  const updateModel = useMutation({
    mutationFn: (next: string) =>
      settingsApi.updateProvider(provider.id, {
        name: provider.name,
        base_url: provider.base_url,
        model: next,
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  // `model` is sent back unchanged: it holds the CLI program id, never user-edited.
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
      className="rounded-card border border-line bg-card p-5 space-y-4"
      data-testid="inactive-provider-card"
    >
      <header className="flex items-center justify-between">
        <h3 className="heading-card">
          {provider.name}
        </h3>
        <Button
          variant="ghost"
          className="px-3 py-1.5 text-[13px] hover:text-straw"
          onClick={() => remove.mutate()}
          disabled={remove.isPending}
          aria-label={`Remove provider ${provider.name}`}
        >
          Remove
        </Button>
      </header>

      {isCli ? (
        <div className="space-y-3">
          <div className="rounded-button bg-paper px-3 py-2 text-[12.5px] text-mut">
            Runs <span className={VALUE}>{provider.model}</span>{" "}
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
                <p role="status" className="text-[12.5px] text-straw">
                  ✗ Could not save model: {String(updateCliModel.error)}
                </p>
              )}
            </Field>
          ) : (
            <p className="text-[12.5px] text-mut">
              Click Test below to confirm the CLI connects - the model
              picker appears once it does.
            </p>
          )}
        </div>
      ) : (
        <>
          <div className="grid grid-cols-2 gap-x-6 gap-y-3">
            <Field label="BASE URL">
              <span className={`${VALUE} break-all`}>
                {provider.base_url}
              </span>
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
                <p role="status" className="text-[12.5px] text-straw">
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
              className="text-[12.5px] font-medium text-blue hover:underline inline-block"
            >
              See all {presetName ?? provider.name} models →
            </a>
          )}
        </>
      )}

      {/* items-start keeps `Set active` pinned to the top when a Test verdict wraps. */}
      <div className="border-t border-line pt-4 mt-4 flex items-start justify-between gap-4">
        <div className="min-w-0 flex-1 space-y-2">
          {isCli ? (
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
              <div className={LABEL}>API KEY · stored locally</div>
              {provider.api_key_masked ? (
                <div className="flex items-center gap-2">
                  <span className={VALUE}>{provider.api_key_masked}</span>
                  <span className="text-matcha text-[12.5px] font-medium">
                    ✓ stored
                  </span>
                </div>
              ) : (
                <div className="text-[12.5px] text-mut">No key stored yet.</div>
              )}
              {keying ? (
                <div className="space-y-2">
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
                  <Button
                    variant="secondary"
                    className="px-3 py-1.5 text-[13px]"
                    onClick={() => {
                      setKeying(false);
                      setApiKeyDraft("");
                      onKeyClosed?.();
                    }}
                  >
                    Cancel
                  </Button>
                </div>
              ) : (
                <div className="flex flex-wrap items-center gap-3">
                  <Button
                    variant="secondary"
                    className="px-3 py-1.5 text-[13px]"
                    onClick={() => setKeying(true)}
                  >
                    {provider.api_key_masked ? "Replace key" : "Add key"}
                  </Button>
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
        <Button
          variant="primary"
          className="shrink-0"
          onClick={() => activate.mutate()}
          disabled={activate.isPending}
        >
          {activate.isPending ? "Activating…" : "Set active"}
        </Button>
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
      <div className={LABEL}>{label}</div>
      <div>{children}</div>
    </div>
  );
}
