import { useMemo, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { settingsApi } from "../../lib/api/settings";
import { ComboBox } from "./ComboBox";
import { Button } from "../Button";

// For `cli` rows, Refresh asks the local binary instead of an HTTP endpoint. That
// catalog is not entitlement-filtered (a free Cursor plan lists every model and
// refuses each at call time), so Refresh shows the whole list and Test says whether a pick works.
// A key draft is sent with Refresh so models can be listed before the key is saved.
// The backend supports a keyless probe for local runtimes, so Refresh is disabled only
// by `disabled` or while a request is in flight.

interface Props {
  providerId: string;
  providerName: string;
  value: string;
  onChange: (next: string) => void;
  onCommit?: (next: string) => void;
  presetModels: string[];
  // Preferred over the stored key for Refresh. Never persisted.
  apiKeyDraft?: string;
  disabled?: boolean;
  // `cli` rows say "CLI default": blank means whatever a bare `claude -p` picks.
  placeholder?: string;
}

export function ModelSelect({
  providerId,
  providerName,
  value,
  onChange,
  onCommit,
  presetModels,
  apiKeyDraft = "",
  disabled,
  placeholder = "e.g. gpt-4o-mini",
}: Props) {
  const [refreshedModels, setRefreshedModels] = useState<string[] | null>(
    null,
  );

  const refresh = useMutation({
    mutationFn: () =>
      settingsApi.listProviderModels(
        providerId,
        apiKeyDraft.trim() || undefined,
      ),
    onSuccess: (models) => setRefreshedModels(models),
  });

  // Live results override the static hint. Memoized so a sibling API KEY keystroke
  // does not hand ComboBox a fresh array every render.
  const suggestions = useMemo(() => {
    const source = refreshedModels ?? presetModels;
    return Array.from(
      new Set([...source, value].filter((m) => m && m.length > 0)),
    );
  }, [refreshedModels, presetModels, value]);

  return (
    <div className="space-y-1">
      <div className="flex items-center gap-2">
        <ComboBox
          value={value}
          onChange={onChange}
          onCommit={onCommit}
          options={suggestions}
          placeholder={placeholder}
          ariaLabel={`Model for ${providerName}`}
          triggerLabel={`Show model list for ${providerName}`}
          disabled={disabled}
        />
        <Button
          variant="secondary"
          onClick={() => refresh.mutate()}
          disabled={disabled || refresh.isPending}
          aria-label={`Refresh model list for ${providerName}`}
          title="Fetch the latest model list from the provider"
          className="px-3 py-1.5 text-[13px] shrink-0"
        >
          {refresh.isPending ? "Refreshing…" : "Refresh"}
        </Button>
      </div>
      {refresh.isError && (
        <p role="status" className="text-[12.5px] text-straw">
          ✗ Could not fetch models: {refresh.error.message}
        </p>
      )}
      {refreshedModels && !refresh.isPending && (
        <p className="text-[12.5px] text-mut">
          Showing {refreshedModels.length} live model
          {refreshedModels.length === 1 ? "" : "s"} from the provider.
        </p>
      )}
    </div>
  );
}
