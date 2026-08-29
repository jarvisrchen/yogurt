import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { settingsApi } from "../../lib/api/settings";

/**
 * AddProviderForm — inline "+ Add" expansion for the Model section
 * (fast task: the `+ Add` button in Settings.tsx used to be dead).
 *
 * No modal library — this renders as an inline card in the same spot the
 * button sat, matching `ProviderCard`'s rounded-xl/border-line/bg-white
 * chrome. `POST /api/settings/providers` inserts the row inactive
 * (`is_active=0`, same as `PresetChip`), so on success it shows up as an
 * inactive `ProviderRow` card in the list — the footer `Set active`
 * action promotes it to `ProviderCard`, whose card UX already handles
 * pasting the key. This form does not duplicate that key UI.
 */

interface Props {
  onDone: () => void;
}

export function AddProviderForm({ onDone }: Props) {
  const qc = useQueryClient();
  const [name, setName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");

  const create = useMutation({
    mutationFn: () =>
      settingsApi.createProvider({
        name: name.trim(),
        base_url: baseUrl.trim(),
        model: model.trim(),
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["settings"] });
      onDone();
    },
  });

  const valid = !!(name.trim() && baseUrl.trim() && model.trim());

  return (
    <form
      data-testid="add-provider-form"
      className="rounded-xl border border-line bg-white p-4 space-y-3"
      onSubmit={(e) => {
        e.preventDefault();
        if (valid && !create.isPending) create.mutate();
      }}
    >
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
        <Field label="NAME">
          <input
            required
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="My provider"
            className="w-full text-[13px] border-b border-line focus:border-[var(--color-blue)] outline-none py-1"
          />
        </Field>
        <Field label="BASE URL">
          <input
            required
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            placeholder="https://…/v1"
            className="w-full font-mono text-[12.5px] border-b border-line focus:border-[var(--color-blue)] outline-none py-1"
          />
        </Field>
        <Field label="MODEL">
          <input
            required
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="gpt-4o-mini"
            className="w-full font-mono text-[12.5px] border-b border-line focus:border-[var(--color-blue)] outline-none py-1"
          />
        </Field>
      </div>

      <div className="flex items-center gap-3">
        <button
          type="submit"
          disabled={!valid || create.isPending}
          className="text-sm font-semibold bg-[var(--color-blue)] text-white px-3 py-1.5 rounded-md disabled:opacity-50"
        >
          {create.isPending ? "Adding…" : "Add provider"}
        </button>
        <button
          type="button"
          onClick={onDone}
          className="text-[12.5px] font-semibold text-mut hover:text-ink"
        >
          Cancel
        </button>
      </div>

      {create.isError && (
        <p className="text-xs text-[var(--color-straw)]">
          Failed to add provider: {String(create.error)}
        </p>
      )}
    </form>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block space-y-1">
      <span className="text-[10px] font-mono uppercase tracking-[0.06em] text-grey block">
        {label}
      </span>
      {children}
    </label>
  );
}
