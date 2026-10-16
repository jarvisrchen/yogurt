import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { settingsApi } from "../../lib/api/settings";
import { Button } from "../Button";
import { LABEL } from "./labelClass";


interface Props {
  onDone: () => void;
  onCreated?: (id: string) => void;
}

export function AddProviderForm({ onDone, onCreated }: Props) {
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
    onSuccess: (created) => {
      qc.invalidateQueries({ queryKey: ["settings"] });
      onCreated?.(created.id);
      onDone();
    },
  });

  const valid = !!(name.trim() && baseUrl.trim() && model.trim());

  return (
    <form
      data-testid="add-provider-form"
      className="rounded-card border border-line bg-card p-4 space-y-3"
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
            className="w-full text-[13.5px] text-ink border-b border-line focus:border-blue outline-none py-1"
          />
        </Field>
        <Field label="BASE URL">
          <input
            required
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            placeholder="https://…/v1"
            className="w-full text-[13.5px] text-ink border-b border-line focus:border-blue outline-none py-1"
          />
        </Field>
        <Field label="MODEL">
          <input
            required
            value={model}
            onChange={(e) => setModel(e.target.value)}
            placeholder="gpt-4o-mini"
            className="w-full text-[13.5px] text-ink border-b border-line focus:border-blue outline-none py-1"
          />
        </Field>
      </div>

      <div className="flex items-center gap-3">
        <Button
          type="submit"
          variant="secondary"
          disabled={!valid || create.isPending}
          className="px-3 py-1.5 text-[13px]"
        >
          {create.isPending ? "Adding…" : "Add provider"}
        </Button>
        <Button
          variant="secondary"
          className="px-3 py-1.5 text-[13px]"
          onClick={onDone}
        >
          Cancel
        </Button>
      </div>

      {create.isError && (
        <p className="text-[12.5px] text-straw">
          Failed to add provider: {String(create.error)}
        </p>
      )}
    </form>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block space-y-1">
      <span className={`${LABEL} block`}>{label}</span>
      {children}
    </label>
  );
}
