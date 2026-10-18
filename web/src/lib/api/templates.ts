// Self-contained rather than built on `./meetings`'s `json` helper, so post-meeting route tests that replace
// that module wholesale still exercise the real fetch here.

import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { bearerFetch } from "../session";

export interface Template {
  id: string;
  name: string;
  when: string;
}

/** The picker's "let the model choose" option; never sent as a template. */
export const AUTO_TEMPLATE = "auto";

export const templatesKey = ["templates"] as const;

export async function fetchTemplates(): Promise<Template[]> {
  const res = await bearerFetch("/api/templates");
  if (!res.ok) throw new Error(`templates: ${res.status} ${res.statusText}`);
  const body: unknown = await res.json();
  return Array.isArray(body) ? (body as Template[]) : [];
}

export function useTemplates(): UseQueryResult<Template[], Error> {
  return useQuery({
    queryKey: templatesKey,
    queryFn: fetchTemplates,
    // The list is baked into the binary; it cannot change under a session.
    staleTime: Infinity,
  });
}
