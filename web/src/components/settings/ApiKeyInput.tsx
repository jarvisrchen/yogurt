import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { settingsApi } from "../../lib/api/settings";

/**
 * Password field + "Test" + "Save key" for one provider's Keychain entry.
 *
 * Shared by `ProviderCard` (always visible on the active provider) and
 * `ProviderRow` (revealed on demand for an inactive one). Before this
 * existed only the ACTIVE card could take a key, so cloning a preset chip
 * left you with a keyless row and no way to fill it — you had to activate
 * the keyless provider first, breaking whatever LLM was working.
 *
 * "Test" runs one real completion against the provider BEFORE the key is
 * committed, which is the point: pasting a key and finding out it was
 * wrong at the next enhance is a bad trade. The draft is sent to the
 * server for the test but never stored by it, so a failed test leaves the
 * provider exactly as it was.
 *
 * The raw key never survives a save: `setDraft("")` runs in `onSuccess`,
 * and the masked view re-renders from the invalidated query rather than
 * from React state.
 */
export function ApiKeyInput({
  providerId,
  providerName,
  hasStoredKey,
  onSaved,
  autoFocus,
  onDraftChange,
}: {
  providerId: string;
  /**
   * Only used for the accessible names. The Settings page can render two of
   * these at once (active card + an expanded inactive row), so a bare
   * "API key" label would be ambiguous to a screen reader.
   */
  providerName: string;
  /**
   * Enables "Test" with an empty field — there is a stored key worth
   * testing. Without a stored key and without a draft there is nothing to
   * test, so the button stays disabled.
   */
  hasStoredKey?: boolean;
  onSaved?: () => void;
  autoFocus?: boolean;
  /**
   * Reports the current draft value (the unmasked key in the password
   * field) up to the parent on every keystroke. Used by the card to
   * pass the draft into the MODEL `Refresh` button so the user can
   * discover available models *before* committing the key — necessary
   * when the saved `model` is the only thing wrong with the provider
   * (Google's frequent deprecations being the canonical case).
   *
   * The draft never reaches this component's onSaved path without an
   * explicit `Save key` click, so the parent's read of it stays
   * read-only from the key store's perspective.
   */
  onDraftChange?: (draft: string) => void;
}) {
  const qc = useQueryClient();
  const [draft, setDraft] = useState("");

  function updateDraft(next: string) {
    setDraft(next);
    onDraftChange?.(next);
  }

  const setKey = useMutation({
    mutationFn: (k: string) => settingsApi.setProviderKey(providerId, k),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["settings"] });
      updateDraft("");
      onSaved?.();
    },
  });

  const test = useMutation({
    // Empty draft → test the stored key instead.
    mutationFn: () => settingsApi.testProvider(providerId, draft || undefined),
  });

  const canTest = (!!draft || !!hasStoredKey) && !test.isPending;
  // Mirror the backend's mask shape (`••••XXXX`, last 4 chars visible) so
  // the user can confirm they pasted the right key BEFORE clicking Save
  // — the password field hides the rest, but a key paste where the last
  // four characters don't match what they copied is a common typo class
  // (trailing whitespace, accidental caps lock, partial paste).
  const previewMasked = draft.length >= 4;
  const previewTail = draft.slice(-4);

  return (
    <div className="space-y-2">
      <div className="flex items-center gap-2">
        <input
          type="password"
          placeholder="Paste new key…"
          aria-label={`API key for ${providerName}`}
          // eslint-disable-next-line jsx-a11y/no-autofocus
          autoFocus={autoFocus}
          className="flex-1 font-mono text-sm border border-line rounded px-2 py-1.5 focus:border-[var(--color-blue)] outline-none"
          value={draft}
          onChange={(e) => {
            updateDraft(e.target.value);
            // A previous verdict describes a key that is no longer in the
            // box — showing a stale green tick next to new text is a lie.
            test.reset();
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && draft && !setKey.isPending) {
              setKey.mutate(draft);
            }
          }}
        />
        <button
          type="button"
          disabled={!canTest}
          aria-label={`Test connection for ${providerName}`}
          className="text-sm font-semibold text-mut border border-line rounded-md px-3 py-1.5 hover:text-ink hover:border-grey disabled:opacity-40 shrink-0"
          onClick={() => test.mutate()}
        >
          {test.isPending ? "Testing…" : "Test"}
        </button>
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

      {draft.length > 0 && (
        <p className="text-[12px] font-mono text-mut">
          Will save as{" "}
          <span className="text-ink">
            {previewMasked ? `••••${previewTail}` : draft}
          </span>
        </p>
      )}

      {test.data && (
        <p
          role="status"
          className={`text-[12.5px] ${
            test.data.ok
              ? "text-[var(--color-matcha)]"
              : "text-[var(--color-straw)]"
          }`}
        >
          {test.data.ok
            ? `✓ Connection works${test.data.model ? ` · answered as ${test.data.model}` : ""}`
            : `✗ ${test.data.error ?? "Connection failed."}`}
        </p>
      )}
      {test.isError && (
        <p role="status" className="text-[12.5px] text-[var(--color-straw)]">
          ✗ Could not reach the yogurt server to run the test.
        </p>
      )}
    </div>
  );
}
