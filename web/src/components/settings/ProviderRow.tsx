import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { ProviderView } from "../../lib/api/settings";
import { settingsApi } from "../../lib/api/settings";
import { ApiKeyInput } from "./ApiKeyInput";

/**
 * Inactive provider card — mirrors `ProviderCard`'s structure so every
 * provider (active or not) reads as a card. The 1.5px blueberry border,
 * ACTIVE badge, soft shadow, and Edit toggle are reserved for the active
 * card (UI-SPEC §Visuals + §Accent reservations #1) — the inactive card
 * uses a regular `--color-line` border, no shadow, and exposes
 * `Set active` as the primary footer action instead of Edit.
 *
 * BASE URL and MODEL are read-only here; editing them on an inactive
 * provider is a separate feature. The API KEY section is collapsible so
 * the card stays compact until the user opts into keying — same shape as
 * before, just lifted into the card's API KEY section so the active and
 * inactive surfaces share a vocabulary.
 */

export function ProviderRow({ provider }: { provider: ProviderView }) {
  const qc = useQueryClient();
  const [keying, setKeying] = useState(false);
  const activate = useMutation({
    mutationFn: () => settingsApi.activateProvider(provider.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  const remove = useMutation({
    mutationFn: () => settingsApi.deleteProvider(provider.id),
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
          <code className="font-mono text-[12.5px] text-ink break-all">
            {provider.model || "—"}
          </code>
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
