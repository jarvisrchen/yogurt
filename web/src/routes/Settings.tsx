import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams } from "react-router";
import { settingsApi } from "../lib/api/settings";
import type { Preset, ProviderView } from "../lib/api/settings";
import { SidebarNav } from "../components/settings/SidebarNav";
import type { SettingsSection } from "../components/settings/SidebarNav";
import { ProviderCard } from "../components/settings/ProviderCard";
import { ProviderRow } from "../components/settings/ProviderRow";
import { PresetChip } from "../components/settings/PresetChip";
import { AddProviderForm } from "../components/settings/AddProviderForm";
import { STTPicker } from "../components/settings/STTPicker";
import { AudioSection } from "../components/settings/AudioSection";
import { GeneralSection } from "../components/settings/GeneralSection";
import { CloudSection } from "../components/settings/cloud/CloudSection";
import { ImportSection } from "../components/settings/ImportSection";
import { LABEL } from "../components/settings/labelClass";

const VALID_SECTIONS = ["model", "transcription", "audio", "general", "cloud", "import"] as const;

function isValidSection(s: unknown): s is SettingsSection {
  return typeof s === "string" && (VALID_SECTIONS as readonly string[]).includes(s);
}

/**
 * `newlyCreatedProviderId` is a one-shot hint: the matching `ProviderRow`
 * mounts with its API key input open. `onKeyClosed` resets it to `null` so
 * a later re-render doesn't re-trigger the auto-open.
 */
export function Settings() {
  const params = useParams<{ section: string; view?: string }>();
  const navigate = useNavigate();
  const section: SettingsSection = isValidSection(params.section)
    ? params.section
    : "model";
  const [addingProvider, setAddingProvider] = useState(false);
  // Cleared eagerly so a rapid second create doesn't auto-open a stale id.
  const [newlyCreatedProviderId, setNewlyCreatedProviderId] = useState<
    string | null
  >(null);
  const q = useQuery({ queryKey: ["settings"], queryFn: settingsApi.get });

  if (q.isLoading) {
    return (
      <div
        className="p-10 space-y-3 max-w-md"
        data-testid="settings-loading"
        aria-busy="true"
      >
        <div className="h-7 w-40 rounded shimmer" />
        <div className="h-5 w-3/4 rounded shimmer" />
        <div className="h-5 w-2/3 rounded shimmer" />
        <div className="h-24 w-full rounded-card shimmer" />
      </div>
    );
  }
  if (q.isError) {
    return (
      <div className="p-10">
        <div className="inline-block text-[13px] text-ink bg-strsoft border border-straw/40 rounded-button px-3 py-2">
          Failed to load: {String(q.error)}
        </div>
      </div>
    );
  }
  const data = q.data!;
  const active = data.providers.find((p) => p.is_active);
  const inactive = data.providers.filter((p) => !p.is_active);
  // `cli` providers have an empty base_url, so they match on `model`, which
  // holds the CLI program id and is never user-edited.
  const findPreset = (provider: ProviderView): Preset | undefined =>
    provider.adapter === "cli"
      ? data.presets.find(
          (p) => p.adapter === "cli" && p.default_model === provider.model,
        )
      : data.presets.find(
          (p) => p.adapter === "http" && p.base_url === provider.base_url,
        );

  return (
    <div className="flex min-h-screen bg-[var(--color-paper)]">
      <SidebarNav
        active={section}
        onChange={(s) => navigate(`/settings/${s}`)}
        providers={data.providers}
        paths={data.paths}
      />
      <main className="flex-1 max-w-3xl px-10 py-8 space-y-10">
        {section === "model" && (
          <section className="space-y-6">
            <header className="space-y-1">
              <div className="flex items-baseline gap-3">
                <h2 className="heading-section">
                  Model
                </h2>
                <code className="text-[11px] text-grey">
                  OpenAI-compatible
                </code>
              </div>
              <p className="text-[13px] text-mut">
                Paste a base URL and key, clone a preset below, or run a
                local agent CLI you already have installed. Anthropic is
                also reachable via OpenRouter.
              </p>
            </header>

            {active ? (
              <ProviderCard
                provider={active}
                presetModels={findPreset(active)?.models ?? []}
                docsUrl={findPreset(active)?.docs_url}
                presetName={findPreset(active)?.name}
                defaultCliModel={findPreset(active)?.default_cli_model ?? ""}
              />
            ) : data.providers.length === 0 ? (
              <div className="rounded-card border border-dashed border-line bg-card/50 p-6 space-y-1">
                <p className="heading-sm">
                  No providers configured yet
                </p>
                <p className="text-[13px] text-mut">
                  Add an OpenAI-compatible endpoint, or use a preset to
                  get started.
                </p>
              </div>
            ) : (
              <p className="text-sm text-mut">
                No active provider - pick one below to set active.
              </p>
            )}

            {inactive.length > 0 && (
              <div data-testid="inactive-providers" className="space-y-3">
                {inactive.map((p) => (
                  <ProviderRow
                    key={p.id}
                    provider={p}
                    autoOpenKey={p.id === newlyCreatedProviderId}
                    presetModels={findPreset(p)?.models ?? []}
                    docsUrl={findPreset(p)?.docs_url}
                    presetName={findPreset(p)?.name}
                    defaultCliModel={findPreset(p)?.default_cli_model ?? ""}
                    onKeyClosed={() => setNewlyCreatedProviderId(null)}
                  />
                ))}
              </div>
            )}

            <div className="pt-4 border-t border-line space-y-3">
              <div className={`${LABEL} mb-2`}>
                Clone a preset
              </div>
              <div className="flex flex-wrap gap-2 items-center">
                {data.presets.map((p) => (
                  <PresetChip
                    key={p.name}
                    preset={p}
                    onCreated={setNewlyCreatedProviderId}
                  />
                ))}
                {!addingProvider && (
                  <button
                    type="button"
                    onClick={() => setAddingProvider(true)}
                    className="inline-flex items-center gap-1.5 rounded-pill px-2.5 py-1 text-[12px] font-medium leading-none border bg-card text-blue border-blue/30 hover:border-blue disabled:opacity-50"
                  >
                    + Add
                  </button>
                )}
              </div>
              {addingProvider && (
                <AddProviderForm
                  onDone={() => setAddingProvider(false)}
                  onCreated={setNewlyCreatedProviderId}
                />
              )}
            </div>
          </section>
        )}

        {section === "transcription" && (
          <section className="space-y-4">
            <h2 className="heading-section">
              Transcription
            </h2>
            <STTPicker />
          </section>
        )}

        {section === "audio" && <AudioSection general={data.general} />}

        {section === "general" && <GeneralSection general={data.general} />}

        {section === "cloud" && <CloudSection view={params.view} />}

        {section === "import" && <ImportSection />}
      </main>
    </div>
  );
}
