import { useMutation } from "@tanstack/react-query";
import { settingsApi, type TestConnectionResult } from "../../lib/api/settings";
import { Button } from "../Button";

// Renders a fragment: the button plus a `basis-full order-last` verdict line, so a
// `flex flex-wrap` parent drops the verdict onto its own last row, even with siblings after the button.
export function TestKeyButton({
  providerId = "",
  providerName,
  // Unsaved key from the password field. Empty tests the stored key.
  draft = "",
  hasStoredKey,
  // A `cli` provider has no key, so Test is always reachable.
  alwaysTestable = false,
  // Override for non-provider callers such as the Deepgram STT key.
  testFn = (key?: string) => settingsApi.testProvider(providerId, key),
  onResult,
  // `false` beside the API key input keeps `Button`'s default sizing to line up with Save key.
  compact = true,
  // Matches a `sm` Save key beside it. Overrides `compact`.
  size,
}: {
  providerId?: string;
  providerName: string;
  draft?: string;
  hasStoredKey?: boolean;
  alwaysTestable?: boolean;
  testFn?: (key?: string) => Promise<TestConnectionResult>;
  onResult?: (result: TestConnectionResult) => void;
  compact?: boolean;
  size?: "sm";
}) {
  const test = useMutation({
    mutationFn: (key: string) => testFn(key || undefined),
    onSuccess: (result) => onResult?.(result),
  });

  // Hide the verdict once the draft is edited, so a green tick never sits beside an untested key.
  const fresh = test.variables === draft;
  const canTest = (alwaysTestable || !!draft || !!hasStoredKey) && !test.isPending;

  return (
    <>
      <Button
        variant="secondary"
        disabled={!canTest}
        aria-label={`Test connection for ${providerName}`}
        size={size}
        className={`shrink-0 ${compact && !size ? "px-3 py-1.5 text-[13px]" : ""}`}
        onClick={() => test.mutate(draft)}
      >
        {test.isPending ? "Testing…" : "Test"}
      </Button>

      {fresh && test.data && (
        <p
          role="status"
          className={`order-last basis-full text-[12.5px] ${
            test.data.ok ? "text-matcha" : "text-straw"
          }`}
        >
          {test.data.ok
            ? `✓ Connection works${test.data.model ? ` · answered as ${test.data.model}` : ""}`
            : `✗ ${test.data.error ?? "Connection failed."}`}
        </p>
      )}
      {fresh && test.isError && (
        <p role="status" className="order-last basis-full text-[12.5px] text-straw">
          ✗ Could not reach the yogurt server to run the test.
        </p>
      )}
    </>
  );
}
