import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { settingsApi } from "../../lib/api/settings";
import { ComboBox } from "./ComboBox";

/**
 * MODEL field with a click-to-open dropdown of available models + a
 * Refresh button.
 *
 * Shared by `ProviderCard` (active) and `ProviderRow` (inactive) so they
 * speak the same vocabulary. The input is wrapped in a `ComboBox` —
 * a clickable dropdown trigger that lists every available model — so
 * the user gets a real "click to see options" affordance (native
 * `<input list>` + `<datalist>` is famously unreliable on Safari/macOS).
 * Free text is always allowed; the dropdown is just a picker over
 * known suggestions.
 *
 * Suggestions come from `presetModels` (the static hint shipped with
 * the preset) and any `refreshedModels` returned by the live
 * `/v1/models` probe. The current model is always in the list so a
 * value the user typed earlier still shows up.
 *
 * `Refresh` calls `POST /api/settings/providers/:id/models`. If the
 * parent passes an `apiKeyDraft` (the unmasked key sitting in the API
 * KEY password field), that draft is sent in the request body and
 * used for the probe — the button is useful *before* the key is saved,
 * which matters when the saved `model` is the only thing wrong with
 * the provider (Google deprecates tiers often enough that this is a
 * real flow). With no draft and no stored key, the button is
 * disabled — anything else would 422 server-side.
 */

interface Props {
  providerId: string;
  providerName: string;
  value: string;
  onChange: (next: string) => void;
  presetModels: string[];
  /** True if there's a key in the Keychain for this provider. */
  hasStoredKey: boolean;
  /**
   * Draft key currently typed in the sibling API KEY field. Preferred
   * over `hasStoredKey` for the `Refresh` call so the user can fetch
   * available models before committing the key. Never persisted.
   */
  apiKeyDraft?: string;
  disabled?: boolean;
}

export function ModelSelect({
  providerId,
  providerName,
  value,
  onChange,
  presetModels,
  hasStoredKey,
  apiKeyDraft = "",
  disabled,
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

  // Either a freshly-pasted key or a stored one lets the user discover
  // models. The draft wins because the user is mid-edit and we want
  // their most recent intent to take effect.
  const canRefresh = hasStoredKey || apiKeyDraft.trim().length > 0;

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
        <ComboBox
          value={value}
          onChange={onChange}
          options={suggestions}
          placeholder="e.g. gpt-4o-mini"
          ariaLabel={`Model for ${providerName}`}
          triggerLabel={`Show model list for ${providerName}`}
          disabled={disabled}
        />
        <button
          type="button"
          onClick={() => refresh.mutate()}
          disabled={!canRefresh || refresh.isPending}
          aria-label={`Refresh model list for ${providerName}`}
          title={
            canRefresh
              ? "Fetch the latest model list from the provider"
              : "Paste an API key above, then refresh"
          }
          className="text-[12.5px] font-semibold text-mut hover:text-ink disabled:opacity-40 disabled:cursor-not-allowed shrink-0"
        >
          {refresh.isPending ? "Refreshing…" : "Refresh"}
        </button>
      </div>
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
