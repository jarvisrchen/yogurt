import { useId, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { settingsApi } from "../../lib/api/settings";

/**
 * MODEL field with autocomplete suggestions + a Refresh button.
 *
 * Shared by `ProviderCard` (active) and `ProviderRow` (inactive) so they
 * speak the same vocabulary. The input is a regular text input paired with
 * a `<datalist>` — the user can type anything (preview models, custom
 * aliases, etc.) and the datalist is just autocomplete, never a constraint.
 *
 * Suggestions come from `presetModels` (the static hint shipped with the
 * preset) and any `refreshedModels` returned by the live `/v1/models`
 * probe. The current model is always in the datalist so a value the user
 * typed earlier still shows up as a suggestion.
 *
 * `Refresh` calls `GET /api/settings/providers/:id/models` with the stored
 * Keychain key. Enabled only when `hasStoredKey` is true; otherwise the
 * button is a no-op affordance (the user needs a key before `/models` will
 * answer, since OpenAI-compatible APIs require Bearer auth).
 */

interface Props {
  providerId: string;
  providerName: string;
  value: string;
  onChange: (next: string) => void;
  presetModels: string[];
  /** True if there's a key in the Keychain for this provider. */
  hasStoredKey: boolean;
  disabled?: boolean;
  /** Visual label inside the wrapping Field, kept here so callers don't
   *  have to render their own label around the input. */
  id?: string;
}

export function ModelSelect({
  providerId,
  providerName,
  value,
  onChange,
  presetModels,
  hasStoredKey,
  disabled,
  id,
}: Props) {
  const reactId = useId();
  const inputId = id ?? `model-${providerId}-${reactId}`;
  const datalistId = `${inputId}-list`;
  const [refreshedModels, setRefreshedModels] = useState<string[] | null>(
    null,
  );

  const refresh = useMutation({
    mutationFn: () => settingsApi.listProviderModels(providerId),
    onSuccess: (models) => setRefreshedModels(models),
  });

  // Order: live results first (they override the static hint), then the
  // preset list, then the current value as a self-suggestion. Dedup,
  // case-sensitive — providers don't agree on casing so we don't try to
  // be clever.
  const source = refreshedModels ?? presetModels;
  const suggestions = Array.from(
    new Set([...source, value].filter((m) => m && m.length > 0)),
  );

  return (
    <div className="space-y-1">
      <div className="flex items-center gap-2">
        <input
          id={inputId}
          list={datalistId}
          disabled={disabled}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder="e.g. gpt-4o-mini"
          className="flex-1 min-w-0 font-mono text-[12.5px] border-b border-line focus:border-[var(--color-blue)] outline-none py-1 disabled:opacity-50"
        />
        <button
          type="button"
          onClick={() => refresh.mutate()}
          disabled={!hasStoredKey || refresh.isPending}
          aria-label={`Refresh model list for ${providerName}`}
          title={
            hasStoredKey
              ? "Fetch the latest model list from the provider"
              : "Save an API key first, then refresh"
          }
          className="text-[12.5px] font-semibold text-mut hover:text-ink disabled:opacity-40 disabled:cursor-not-allowed shrink-0"
        >
          {refresh.isPending ? "Refreshing…" : "Refresh"}
        </button>
      </div>
      <datalist id={datalistId}>
        {suggestions.map((m) => (
          <option key={m} value={m} />
        ))}
      </datalist>
      {refresh.isError && (
        <p role="status" className="text-[11px] text-[var(--color-straw)]">
          ✗ Could not fetch models: {String(refresh.error)}
        </p>
      )}
      {refreshedModels && !refresh.isPending && (
        <p className="text-[11px] text-mut">
          Showing {refreshedModels.length} live model
          {refreshedModels.length === 1 ? "" : "s"} from the provider.
        </p>
      )}
    </div>
  );
}
