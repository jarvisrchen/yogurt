import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate, useParams } from "react-router";
import { settingsApi } from "../lib/api/settings";
import type { Preset } from "../lib/api/settings";
import { SidebarNav } from "../components/settings/SidebarNav";
import type { SettingsSection } from "../components/settings/SidebarNav";
import { ProviderCard } from "../components/settings/ProviderCard";
import { ProviderRow } from "../components/settings/ProviderRow";
import { PresetChip } from "../components/settings/PresetChip";
import { AddProviderForm } from "../components/settings/AddProviderForm";
import { STTPicker } from "../components/settings/STTPicker";
import { AudioSection } from "../components/settings/AudioSection";
import { GeneralSection } from "../components/settings/GeneralSection";

const VALID_SECTIONS = ["model", "transcription", "audio", "general"] as const;

function isValidSection(s: unknown): s is SettingsSection {
  return typeof s === "string" && (VALID_SECTIONS as readonly string[]).includes(s);
}

/**
 * `/settings/:section` page — Phase 5 (Plan 05-03), satisfies SET-01..SET-06.
 *
 * Layout: 212px sidebar + flexible main content. The Model section is
 * fully wired to `/api/settings*` in this plan; Transcription / Audio /
 * General render placeholder text — those sections land in plan 05-04.
 *
 * State strategy:
 * - `useQuery(["settings"])` is the single source of truth for the entire
 *   page. Every mutation (`updateProvider`, `setProviderKey`,
 *   `activateProvider`, `deleteProvider`, `createProvider`) invalidates
 *   that key, triggering a single refetch + cascaded re-render.
 * - Section selection is URL-driven (`/settings/:section`). Reading
 *   `useParams` keeps a refresh on the same surface; clicking a sidebar
 *   item pushes a new path rather than mutating local state.
 * - `newlyCreatedProviderId` is a one-shot hint: when `createProvider`
 *   returns, the chip/form hands the new id up here, and the matching
 *   `ProviderRow` mounts with its API key input already expanded and
 *   focused so the user lands at the cursor instead of clicking again.
 *   The hint is overwritten on the next create; non-matching rows see
 *   `autoOpenKey={false}` and behave normally.
 * - Falls back to "model" when `:section` is missing or invalid so the
 *   component still renders sensibly if mounted outside the route table
 *   (e.g. older tests).
 */
export function Settings() {
  const params = useParams<{ section: string }>();
  const navigate = useNavigate();
  const section: SettingsSection = isValidSection(params.section)
    ? params.section
    : "model";
  const [addingProvider, setAddingProvider] = useState(false);
  // Tracks the most recently created provider so its card can auto-open
  // the API key input. Reset on the next user action that suggests the
  // auto-open was consumed (the row's own dismiss/save, or starting a new
  // form). Cleared eagerly here so a rapid second create doesn't point
  // the auto-open at a stale id.
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
  // Match a saved provider to its built-in preset by base_url so the card
  // can show the preset's model hint and docs link. The match is by URL
  // only — if the user pastes a custom URL we just don't show the hint,
  // which is the same behavior as before the preset metadata existed.
  const findPreset = (baseUrl: string): Preset | undefined =>
    data.presets.find((p) => p.base_url === baseUrl);

  return (
    <div className="flex min-h-screen bg-[var(--color-paper)]">
      <SidebarNav
        active={section}
        onChange={(s) => navigate(`/settings/${s}`)}
        providers={data.providers}
      />
      <main className="flex-1 max-w-3xl px-10 py-8 space-y-10">
        {section === "model" && (
          <section className="space-y-6">
            <header className="space-y-1">
              <div className="flex items-baseline gap-3">
                <h2 className="font-serif text-[28px] leading-none">
                  Model
                </h2>
                <code className="text-[11px] font-mono text-grey">
                  OpenAI-compatible
                </code>
              </div>
              <p className="text-[13px] text-mut">
                Paste a base URL and key, or clone a preset below. Anthropic
                is reachable via OpenRouter.
              </p>
            </header>

            {active ? (
              <ProviderCard
                provider={active}
                presetModels={findPreset(active.base_url)?.models ?? []}
                docsUrl={findPreset(active.base_url)?.docs_url}
                presetName={findPreset(active.base_url)?.name}
              />
            ) : data.providers.length === 0 ? (
              <div className="rounded-xl border border-dashed border-line bg-white/50 p-6 space-y-1">
                <p className="font-serif text-[18px] text-ink">
                  No providers configured yet
                </p>
                <p className="text-[13px] text-mut">
                  Add an OpenAI-compatible endpoint, or use a preset to
                  get started.
                </p>
              </div>
            ) : (
              <p className="text-sm text-mut">
                No active provider — pick one below to set active.
              </p>
            )}

            {inactive.length > 0 && (
              <div data-testid="inactive-providers">
                {inactive.map((p) => (
                  <ProviderRow
                    key={p.id}
                    provider={p}
                    autoOpenKey={p.id === newlyCreatedProviderId}
                    presetModels={findPreset(p.base_url)?.models ?? []}
                    docsUrl={findPreset(p.base_url)?.docs_url}
                    presetName={findPreset(p.base_url)?.name}
                  />
                ))}
              </div>
            )}

            <div className="pt-4 border-t border-dashed border-line space-y-3">
              <div className="text-[10px] font-mono uppercase tracking-[0.06em] text-grey mb-2">
                Clone a preset →
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
                    className="text-[12.5px] font-semibold text-[var(--color-blue)] hover:underline"
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
            <h2 className="font-serif text-[28px] leading-none">
              Transcription
            </h2>
            <STTPicker />
          </section>
        )}

        {section === "audio" && <AudioSection general={data.general} />}

        {section === "general" && <GeneralSection general={data.general} />}
      </main>
    </div>
  );
}
