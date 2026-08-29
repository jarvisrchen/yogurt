import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { settingsApi } from "../../lib/api/settings";

/**
 * Password field + "Save key" button for one provider's Keychain entry.
 *
 * Shared by `ProviderCard` (always visible on the active provider) and
 * `ProviderRow` (revealed on demand for an inactive one). Before this
 * existed only the ACTIVE card could take a key, so cloning a preset chip
 * left you with a keyless row and no way to fill it — you had to activate
 * the keyless provider first, breaking whatever LLM was working.
 *
 * The raw key never survives the save: `setKeyDraft("")` runs in
 * `onSuccess`, and the masked view re-renders from the invalidated query
 * rather than from React state.
 */
export function ApiKeyInput({
  providerId,
  providerName,
  onSaved,
  autoFocus,
}: {
  providerId: string;
  /**
   * Only used for the accessible names. The Settings page can render two of
   * these at once (active card + an expanded inactive row), so a bare
   * "API key" label would be ambiguous to a screen reader.
   */
  providerName: string;
  onSaved?: () => void;
  autoFocus?: boolean;
}) {
  const qc = useQueryClient();
  const [draft, setDraft] = useState("");
  const setKey = useMutation({
    mutationFn: (k: string) => settingsApi.setProviderKey(providerId, k),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["settings"] });
      setDraft("");
      onSaved?.();
    },
  });

  return (
    <div className="flex items-center gap-2">
      <input
        type="password"
        placeholder="Paste new key…"
        aria-label={`API key for ${providerName}`}
        // eslint-disable-next-line jsx-a11y/no-autofocus
        autoFocus={autoFocus}
        className="flex-1 font-mono text-sm border border-line rounded px-2 py-1.5 focus:border-[var(--color-blue)] outline-none"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && draft && !setKey.isPending) {
            setKey.mutate(draft);
          }
        }}
      />
      <button
        type="button"
        disabled={!draft || setKey.isPending}
        aria-label={`Save API key for ${providerName}`}
        className="text-sm font-semibold bg-[var(--color-blue)] text-white px-3 py-1.5 rounded-md disabled:opacity-50 shrink-0"
        onClick={() => setKey.mutate(draft)}
      >
        {setKey.isPending ? "Saving…" : "Save key"}
      </button>
    </div>
  );
}
