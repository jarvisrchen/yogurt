import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { ProviderView } from "../../lib/api/settings";
import { settingsApi } from "../../lib/api/settings";
import { ApiKeyInput } from "./ApiKeyInput";

/**
 * Inactive provider stacked row — Phase 5 (Plan 05-03), SET-05.
 *
 * Renders the name, base_url in mono, a key-presence indicator (`✓ key`
 * matcha vs `no key` neutral), and action links: `Add key` / `Replace key`,
 * blueberry `Set active` (UI-SPEC §Accent reservations #4) and a muted
 * `Remove` link that turns strawberry on hover.
 *
 * The key affordance exists because cloning a preset chip creates an
 * INACTIVE row, and originally only the active card could accept a key —
 * so the only way to key a freshly-cloned provider was to activate it
 * first, knocking out whatever LLM was currently working.
 *
 * Clicking `Set active` invokes the atomic `POST /api/settings/providers/:id/activate`
 * — the server's `set_active` transaction guarantees no flicker / no
 * two-active state (UI-SPEC §Interaction 2).
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
    <div className="border-b border-line py-3">
      {/* gap-4 is load-bearing: a long base_url truncates and eats every
       * pixel of slack, at which point justify-between leaves the key
       * indicator butted right up against the "Add key" link. */}
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-baseline gap-3 min-w-0">
          <span className="text-[14px] font-semibold text-ink shrink-0">
            {provider.name}
          </span>
          <code className="font-mono text-xs text-grey truncate">
            {provider.base_url}
          </code>
          {provider.api_key_masked ? (
            <span className="text-xs text-[var(--color-matcha)] font-mono font-semibold shrink-0">
              ✓ key
            </span>
          ) : (
            <span className="text-xs text-mut font-mono shrink-0">no key</span>
          )}
        </div>
        <div className="flex items-center gap-4 text-[12.5px] shrink-0">
          <button
            type="button"
            className="text-mut font-semibold hover:text-ink"
            onClick={() => setKeying((k) => !k)}
          >
            {keying
              ? "Cancel"
              : provider.api_key_masked
                ? "Replace key"
                : "Add key"}
          </button>
          <button
            type="button"
            className="text-[var(--color-blue)] font-semibold hover:underline disabled:opacity-50"
            onClick={() => activate.mutate()}
            disabled={activate.isPending}
          >
            Set active
          </button>
          <button
            type="button"
            className="text-mut hover:text-[var(--color-straw)] disabled:opacity-50"
            onClick={() => remove.mutate()}
            disabled={remove.isPending}
          >
            Remove
          </button>
        </div>
      </div>

      {keying && (
        <div className="pt-3">
          <ApiKeyInput
            providerId={provider.id}
            providerName={provider.name}
            autoFocus
            onSaved={() => setKeying(false)}
          />
        </div>
      )}
    </div>
  );
}
