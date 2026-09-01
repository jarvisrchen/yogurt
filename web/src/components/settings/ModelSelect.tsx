import { useMemo, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { settingsApi } from "../../lib/api/settings";
import { ComboBox } from "./ComboBox";

/**
 * MODEL field with a click-to-open dropdown of available models + a
 * Refresh button.
 *
 * Shared by `ProviderCard` (active) and `ProviderRow` (inactive) so they
 * speak the same vocabulary. The input is wrapped in a `ComboBox` -
 * a clickable dropdown trigger that lists every available model - so
 * the user gets a real "click to see options" affordance (native
 * `<input list>` + `<datalist>` is famously unreliable on Safari/macOS).
 * Free text is always allowed; the dropdown is just a picker over
 * known suggestions.
 *
 * Suggestions come from `presetModels` (the static hint shipped with
 * the preset) and any `refreshedModels` returned by the live probe. The
 * current model is always in the list so a value the user typed earlier
 * still shows up.
 *
 * Shared with `cli`-adapter rows, where the same `Refresh` asks the
 * local binary (`cursor-agent --list-models`) instead of an HTTP
 * endpoint. That catalog is not entitlement-filtered - a free Cursor
 * plan lists every model and then refuses each named one at call time -
 * so Refresh deliberately shows the whole list and lets `Test` be the
 * thing that says whether a pick actually works.
 *
 * `Refresh` calls `POST /api/settings/providers/:id/models`. If the
 * parent passes an `apiKeyDraft` (the unmasked key sitting in the API
 * KEY field), that draft is sent in the request body and used for the
 * probe - the button is useful *before* the key is saved, which matters
 * when the saved `model` is the only thing wrong with the provider
 * (Google deprecates tiers often enough that this is a real flow). The
 * backend supports a keyless probe for local runtimes (Ollama, LM
 * Studio) that need no key at all, so Refresh is only ever disabled by
 * `disabled` (the field is read-only) or while a request is in flight.
 */

interface Props {
  providerId: string;
  providerName: string;
  value: string;
  onChange: (next: string) => void;
  /** Fires once when the user commits to a model - picking an option,
   *  pressing Enter, or blurring - as opposed to `onChange`'s per-
   *  keystroke updates. */
  onCommit?: (next: string) => void;
  presetModels: string[];
  /**
   * Draft key currently typed in the sibling API KEY field. Preferred
   * over the stored key for the `Refresh` call so the user can fetch
   * available models before committing the key. Never persisted.
   */
  apiKeyDraft?: string;
  disabled?: boolean;
  /** Empty-field hint. `cli` rows say "CLI default" - leaving it blank
   *  there means "whatever a bare `claude -p` picks", not "unset". */
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

  // Order: live results first (they override the static hint), then the
  // preset list, then the current value as a self-suggestion. Dedup,
  // case-sensitive - providers don't agree on casing so we don't try to
  // be clever. Memoized so a sibling keystroke (the API KEY field bubbles
  // its draft up through the parent) doesn't hand `ComboBox` a fresh
  // array every render.
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
        <button
          type="button"
          onClick={() => refresh.mutate()}
          disabled={disabled || refresh.isPending}
          aria-label={`Refresh model list for ${providerName}`}
          title="Fetch the latest model list from the provider"
          className="text-[12.5px] font-semibold text-mut hover:text-ink disabled:opacity-40 disabled:cursor-not-allowed shrink-0"
        >
          {refresh.isPending ? "Refreshing…" : "Refresh"}
        </button>
      </div>
      {refresh.isError && (
        <p role="status" className="text-[11px] text-[var(--color-straw)]">
          ✗ Could not fetch models: {refresh.error.message}
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
