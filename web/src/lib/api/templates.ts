/**
 * `GET /api/templates` - the note formats enhance can shape a summary
 * into (`crates/yogurt-prompts/templates/enhance/*.md`), in picker order.
 *
 * Self-contained rather than built on `./meetings`'s `json` helper so the
 * post-meeting route's tests, which replace that module wholesale, still
 * exercise the real fetch here.
 */

import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { ensureSessionToken } from "../session";

/** Wire shape of one row, matching `yogurt_prompts::Template`. */
export interface Template {
  id: string;
  /** Display name, e.g. "Design review". */
  name: string;
  /** One line on when the format fits - the picker's tooltip. */
  when: string;
}

/** The picker's "let the model choose" option; never sent as a template. */
export const AUTO_TEMPLATE = "auto";

export const templatesKey = ["templates"] as const;

export async function fetchTemplates(): Promise<Template[]> {
  const token = await ensureSessionToken();
  const res = await fetch("/api/templates", {
    headers: { Authorization: `Bearer ${token}` },
  });
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
