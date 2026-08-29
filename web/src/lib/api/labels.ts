/**
 * Typed fetch wrappers + TanStack-Query hooks for the Granola-style
 * meeting labels REST surface (`/api/labels*` from
 * `crates/yogurt-server/src/api/labels.rs`).
 */

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { json, meetingsKey, type Label, type LabelColor } from "./meetings";

export type { Label, LabelColor };

export interface LabelWithCount extends Label {
  meeting_count: number;
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
};

/** `GET /api/labels`. */
export function useLabels(): UseQueryResult<LabelWithCount[], Error> {
  return useQuery({
    queryKey: labelsKey,
    queryFn: labelsApi.list,
    staleTime: 5_000,
  });
}

/** `POST /api/labels` — find-or-create. */
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

/** `PATCH /api/labels/:id` — rename / recolor. */
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

/** `DELETE /api/labels/:id`. */
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
