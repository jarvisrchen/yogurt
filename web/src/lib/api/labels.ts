import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { json, meetingsKey, type Label, type LabelColor, type PaletteColor } from "./meetings";

export type { Label, LabelColor, PaletteColor };

export interface LabelWithCount extends Label {
  meeting_count: number;
  position: number;
  /** Unix ms, bumped on rename/recolor and on apply/remove. Backs the "Last updated" sort. */
  updated_at: number;
}

export const labelsKey = ["labels"] as const;

export const labelsApi = {
  list: () => json<LabelWithCount[]>("/api/labels"),
  create: (name: string, color?: LabelColor) =>
    json<Label>("/api/labels", {
      method: "POST",
      body: JSON.stringify(color ? { name, color } : { name }),
    }),
  update: (id: string, patch: { name?: string; color?: LabelColor }) =>
    json<Label>(`/api/labels/${id}`, {
      method: "PATCH",
      body: JSON.stringify(patch),
    }),
  delete: (id: string) => json<void>(`/api/labels/${id}`, { method: "DELETE" }),
  reorder: (ids: string[]) =>
    json<void>("/api/labels/order", {
      method: "PUT",
      body: JSON.stringify({ ids }),
    }),
};

export function useLabels(): UseQueryResult<LabelWithCount[], Error> {
  return useQuery({
    queryKey: labelsKey,
    queryFn: labelsApi.list,
    staleTime: 5_000,
  });
}

/** Find-or-create. */
export function useCreateLabel(): UseMutationResult<
  Label,
  Error,
  { name: string; color?: LabelColor }
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ name, color }) => labelsApi.create(name, color),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: labelsKey });
      qc.invalidateQueries({ queryKey: meetingsKey });
    },
  });
}

export function useUpdateLabel(): UseMutationResult<
  Label,
  Error,
  { id: string; name?: string; color?: LabelColor }
> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...patch }) => labelsApi.update(id, patch),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: labelsKey });
      // A rename/recolor changes every meeting's embedded label copy.
      qc.invalidateQueries({ queryKey: meetingsKey });
    },
  });
}

export function useDeleteLabel(): UseMutationResult<void, Error, string> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => labelsApi.delete(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: labelsKey });
      qc.invalidateQueries({ queryKey: meetingsKey });
    },
  });
}

/**
 * Optimistic: applies the new order immediately so the drag does not snap back.
 * `onSettled` refetches to correct a failed request or a concurrent change.
 */
export function useReorderLabels(): UseMutationResult<void, Error, string[]> {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (ids: string[]) => labelsApi.reorder(ids),
    onMutate: (ids) => {
      const prev = qc.getQueryData<LabelWithCount[]>(labelsKey);
      if (!prev) return;
      const byId = new Map(prev.map((l) => [l.id, l]));
      qc.setQueryData(
        labelsKey,
        ids.flatMap((id, position) => {
          const l = byId.get(id);
          return l ? [{ ...l, position }] : [];
        }),
      );
    },
    onSettled: () => qc.invalidateQueries({ queryKey: labelsKey }),
  });
}
