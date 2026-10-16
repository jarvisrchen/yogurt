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

// The 1.5px blueberry border marks the live LLM. The half-step is intentional, do not round.

interface Props {
  provider: ProviderView;
}

export function ProviderCard({
  provider,
  presetModels = [],
  docsUrl,
  presetName,
  defaultCliModel = "",
}: Props & {
  presetModels?: string[];
  docsUrl?: string;
  // Visible text of the docs link. The user-chosen `provider.name` would read badly there.
  presetName?: string;
  // `cli` only: seeded into `cli_model` the first time a Test proves the CLI connects.
  defaultCliModel?: string;
}) {
  const isCli = provider.adapter === "cli";
  const qc = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState({
    name: provider.name,
    base_url: provider.base_url,
    model: provider.model,
  });
  // Lifted out of `ApiKeyInput` so Refresh can probe with it before Save key.
  const [apiKeyDraft, setApiKeyDraft] = useState("");
  // `cli` rows have no Edit/Save flow, so the model picker PATCHes on commit.
  const [cliModelDraft, setCliModelDraft] = useState(provider.cli_model);
  useEffect(() => {
    setCliModelDraft(provider.cli_model);
  }, [provider.cli_model]);
  const [cliTestedOk, setCliTestedOk] = useState(false);
  const cliVerified = !!provider.cli_model || cliTestedOk;

  const update = useMutation({
    mutationFn: () => settingsApi.updateProvider(provider.id, draft),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["settings"] });
      setEditing(false);
    },
  });
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
      className="rounded-card border-[1.5px] border-blue bg-card p-5 shadow-button-blue space-y-4"
      data-testid="active-provider-card"
    >
      <header className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <h3 className="heading-card">
            {provider.name}
          </h3>
          <span className="text-[11px] font-semibold uppercase tracking-[0.06em] bg-blsoft text-blue px-2 py-0.5 rounded">
            Active
          </span>
        </div>
        {!isCli && (
          <Button
            variant="ghost"
            className="px-3 py-1.5 text-[13px]"
            onClick={() => setEditing((e) => !e)}
          >
            {editing ? "Cancel" : "Edit"}
          </Button>
        )}
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
              {editing ? (
                <input
                  className="w-full text-[13.5px] text-ink border-b border-line focus:border-blue outline-none py-1"
                  value={draft.base_url}
                  onChange={(e) =>
                    setDraft({ ...draft, base_url: e.target.value })
                  }
                />
              ) : (
                <span className={`${VALUE} break-all`}>
                  {provider.base_url}
                </span>
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
                  apiKeyDraft={apiKeyDraft}
                />
              ) : (
                <span className={VALUE}>{provider.model || "—"}</span>
              )}
            </Field>
          </div>
          {/* One flex row: as bare inline siblings the docs link and Save would touch with no gap. */}
          {(docsUrl || editing) && (
            <div className="flex flex-wrap items-center gap-3">
              {docsUrl && (
                <a
                  href={docsUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[12.5px] font-medium text-blue hover:underline"
                >
                  See all {presetName ?? provider.name} models →
                </a>
              )}

              {editing && (
                <Button
                  variant="secondary"
                  className="px-3 py-1.5 text-[13px]"
                  disabled={update.isPending}
                  onClick={() => update.mutate()}
                >
                  {update.isPending ? "Saving…" : "Save"}
                </Button>
              )}
            </div>
          )}
        </>
      )}

      <div className="border-t border-line pt-4 mt-4 space-y-2">
        {isCli ? (
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
            <div className="pt-1">
              <ApiKeyInput
                providerId={provider.id}
                providerName={provider.name}
                hasStoredKey={!!provider.api_key_masked}
                onDraftChange={setApiKeyDraft}
              />
            </div>
          </>
        )}
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
