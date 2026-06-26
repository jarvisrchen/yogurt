import { useMutation, useQueryClient } from "@tanstack/react-query";
import type { ProviderView } from "../../lib/api/settings";
import { settingsApi } from "../../lib/api/settings";

/**
 * Inactive provider stacked row — Phase 5 (Plan 05-03), SET-05.
 *
 * Renders the name, base_url in mono, a key-presence indicator (`✓ key`
 * matcha vs `no key` neutral), and two action links: blueberry
 * `Set active` (UI-SPEC §Accent reservations #4) and a muted `Remove`
 * link that turns strawberry on hover.
 *
 * Clicking `Set active` invokes the atomic `POST /api/settings/providers/:id/activate`
 * — the server's `set_active` transaction guarantees no flicker / no
 * two-active state (UI-SPEC §Interaction 2).
 */

export function ProviderRow({ provider }: { provider: ProviderView }) {
  const qc = useQueryClient();
  const activate = useMutation({
    mutationFn: () => settingsApi.activateProvider(provider.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });
  const remove = useMutation({
    mutationFn: () => settingsApi.deleteProvider(provider.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["settings"] }),
  });

  return (
    <div className="flex items-center justify-between border-b border-neutral-200 py-3">
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
          <span className="text-xs text-neutral-400 font-mono shrink-0">
            no key
          </span>
        )}
      </div>
      <div className="flex items-center gap-4 text-[12.5px] shrink-0">
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
          className="text-neutral-400 hover:text-[var(--color-straw)] disabled:opacity-50"
          onClick={() => remove.mutate()}
          disabled={remove.isPending}
        >
          Remove
        </button>
      </div>
    </div>
  );
}
