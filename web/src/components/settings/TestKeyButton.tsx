import { useMutation } from "@tanstack/react-query";
import { settingsApi, type TestConnectionResult } from "../../lib/api/settings";

/**
 * "Test" button + its verdict line for one provider's key.
 *
 * Extracted out of `ApiKeyInput` because `ProviderRow` keeps the key
 * section collapsed until the user clicks `Add key` / `Replace key` —
 * which made `Test` unreachable for exactly the providers most worth
 * testing: the ones already set up, with a stored key, quietly failing.
 * Collapsed cards now render this on its own, testing the stored key.
 *
 * Renders as a fragment: the button plus a `basis-full` verdict line, so
 * a `flex flex-wrap` parent drops the verdict onto its own row underneath
 * instead of squeezing it into the button row.
 */
export function TestKeyButton({
  providerId = "",
  providerName,
  /** Unsaved key from the password field. Empty → test the stored key. */
  draft = "",
  /** Without a stored key and without a draft there is nothing to test. */
  hasStoredKey,
  /** LLM-4: a `cli`-adapter provider has no key at all, so the normal
   *  "needs a draft or a stored key" gate never applies to it - `Test`
   *  is always reachable (it just resolves the CLI on `$PATH`). */
  alwaysTestable = false,
  /** Override for non-provider callers (e.g. the Deepgram STT key), which
   *  hit a different endpoint with no provider id. Defaults to testing
   *  `providerId` against the LLM provider endpoint. */
  testFn = (key?: string) => settingsApi.testProvider(providerId, key),
  /** Fires with every test verdict (ok or not) - LLM-4's cli rows use this
   *  to reveal the MODEL picker once a test actually proves the CLI
   *  connects, instead of showing it unconditionally. */
  onResult,
}: {
  providerId?: string;
  providerName: string;
  draft?: string;
  hasStoredKey?: boolean;
  alwaysTestable?: boolean;
  testFn?: (key?: string) => Promise<TestConnectionResult>;
  onResult?: (result: TestConnectionResult) => void;
}) {
  const test = useMutation({
    mutationFn: (key: string) => testFn(key || undefined),
    onSuccess: (result) => onResult?.(result),
  });

  // A verdict describes the key that was in the box when it ran; a green
  // tick sitting next to text the user has since edited is a lie, so the
  // verdict is only shown while the draft still matches what was tested.
  const fresh = test.variables === draft;
  const canTest = (alwaysTestable || !!draft || !!hasStoredKey) && !test.isPending;

  return (
    <>
      <button
        type="button"
        disabled={!canTest}
        aria-label={`Test connection for ${providerName}`}
        className="text-sm font-semibold text-mut border border-line rounded-md px-3 py-1.5 hover:text-ink hover:border-grey disabled:opacity-40 shrink-0"
        onClick={() => test.mutate(draft)}
      >
        {test.isPending ? "Testing…" : "Test"}
      </button>

      {fresh && test.data && (
        <p
          role="status"
          className={`basis-full text-[12.5px] ${
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
      {fresh && test.isError && (
        <p
          role="status"
          className="basis-full text-[12.5px] text-[var(--color-straw)]"
        >
          ✗ Could not reach the yogurt server to run the test.
        </p>
      )}
    </>
  );
}
