import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { settingsApi } from "../lib/api/settings";
import { SidebarNav } from "../components/settings/SidebarNav";
import type { SettingsSection } from "../components/settings/SidebarNav";
import { ProviderCard } from "../components/settings/ProviderCard";
import { ProviderRow } from "../components/settings/ProviderRow";
import { PresetChip } from "../components/settings/PresetChip";

/**
 * `/settings` page — Phase 5 (Plan 05-03), satisfies SET-01..SET-06.
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
 * - Section selection is local UI state (`useState<Section>`) — it doesn't
 *   round-trip the server.
 */
export function Settings() {
  const [section, setSection] = useState<SettingsSection>("model");
  const q = useQuery({ queryKey: ["settings"], queryFn: settingsApi.get });

  if (q.isLoading) {
    return (
      <div className="p-10 text-mut" data-testid="settings-loading">
        Loading settings…
      </div>
    );
  }
  if (q.isError) {
    return (
      <div className="p-10 text-[var(--color-straw)]">
        Failed to load: {String(q.error)}
      </div>
    );
  }
  const data = q.data!;
  const active = data.providers.find((p) => p.is_active);
  const inactive = data.providers.filter((p) => !p.is_active);

  return (
    <div className="flex min-h-screen bg-[var(--color-paper)]">
      <SidebarNav
        active={section}
        onChange={setSection}
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
                Paste a base URL and key. Anthropic &amp; Gemini reachable
                via OpenRouter.
              </p>
            </header>

            {active ? (
              <ProviderCard provider={active} />
            ) : data.providers.length === 0 ? (
              <div className="rounded-xl border border-dashed border-neutral-300 bg-white/50 p-6 space-y-1">
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
                  <ProviderRow key={p.id} provider={p} />
                ))}
              </div>
            )}

            <div className="pt-4 border-t border-dashed border-neutral-300">
              <div className="text-[10px] font-mono uppercase tracking-[0.06em] text-grey mb-2">
                Clone a preset →
              </div>
              <div className="flex flex-wrap gap-2 items-center">
                {data.presets.map((p) => (
                  <PresetChip key={p.name} preset={p} />
                ))}
                <button
                  type="button"
                  className="text-[12.5px] font-semibold text-[var(--color-blue)] hover:underline"
                >
                  + Add
                </button>
              </div>
            </div>
          </section>
        )}

        {section === "transcription" && (
          <section className="space-y-3">
            <h2 className="font-serif text-[28px] leading-none">
              Transcription
            </h2>
            <p className="text-[13px] text-mut">
              Coming up in plan 05-04.
            </p>
          </section>
        )}

        {section === "audio" && (
          <section className="space-y-3">
            <h2 className="font-serif text-[28px] leading-none">Audio</h2>
            <p className="text-[13px] text-mut">
              Coming up in plan 05-04.
            </p>
          </section>
        )}

        {section === "general" && (
          <section className="space-y-3">
            <h2 className="font-serif text-[28px] leading-none">General</h2>
            <p className="text-[13px] text-mut">
              Coming up in plan 05-04.
            </p>
          </section>
        )}
      </main>
    </div>
  );
}
