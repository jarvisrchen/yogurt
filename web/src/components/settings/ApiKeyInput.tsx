import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { settingsApi } from "../../lib/api/settings";
import { TestKeyButton } from "./TestKeyButton";
import { Button } from "../Button";

// The raw key never survives a save: the draft clears in onSuccess and the
// masked view re-renders from the invalidated query.
export function ApiKeyInput({
  providerId,
  providerName,
  hasStoredKey,
  onSaved,
  autoFocus,
  onDraftChange,
}: {
  providerId: string;
  // Used for accessible names: two of these can render at once, so a bare "API key" label is ambiguous.
  providerName: string;
  // Enables "Test" with an empty field when a stored key exists to test.
  hasStoredKey?: boolean;
  onSaved?: () => void;
  autoFocus?: boolean;
  // Lets the parent pass the unsaved draft to the model Refresh button, so models
  // can be listed before the key is committed.
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

  // Mirrors the backend mask (last 4 chars) so a mistyped paste is visible before Save.
  const previewMasked = draft.length >= 4;
  const previewTail = draft.slice(-4);

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        <input
          type="password"
          placeholder="Paste new key…"
          aria-label={`API key for ${providerName}`}
          // eslint-disable-next-line jsx-a11y/no-autofocus
          autoFocus={autoFocus}
          className="flex-1 rounded-button border border-line bg-paper px-3 py-2 text-[13.5px] font-mono text-ink placeholder:text-grey placeholder:font-sans focus:border-blue focus:outline-none"
          value={draft}
          onChange={(e) => updateDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && draft && !setKey.isPending) {
              setKey.mutate(draft);
            }
          }}
        />
        <TestKeyButton
          providerId={providerId}
          providerName={providerName}
          draft={draft}
          hasStoredKey={hasStoredKey}
          compact={false}
        />
        <Button
          variant="primary"
          disabled={!draft || setKey.isPending}
          aria-label={`Save API key for ${providerName}`}
          className="shrink-0"
          onClick={() => setKey.mutate(draft)}
        >
          {setKey.isPending ? "Saving…" : "Save key"}
        </Button>
      </div>

      {draft.length > 0 && (
        <p className="text-[12.5px] text-mut">
          Will save as{" "}
          <span className="text-ink font-mono">
            {previewMasked ? `••••${previewTail}` : draft}
          </span>
        </p>
      )}

    </div>
  );
}
