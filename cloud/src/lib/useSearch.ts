import { useEffect, useState } from "react";
import { TokenRejected, type Api, type SearchHit } from "./api";
import { SEARCH_PAGE } from "./search";

const DEBOUNCE_MS = 250;

interface Outcome {
  q: string;
  device: string | null;
  offset: number;
  hits?: SearchHit[];
  error?: string;
}

/**
 * Server search for `query`, optionally within one device.
 * An outcome is stored with the query it answers, so a reply to an older input is never shown as the answer to the current one, and `pending` covers the debounce and the request.
 * `previous` is the last answer, even an empty one, while the next one is pending, for showing it dimmed. It is dropped when the field is cleared or the device changes.
 * Results come a page at a time. `offset` is the first hit of the page shown, and a new query or device starts again at the first page.
 */
export function useSearch(api: Api, token: string, query: string, device: string | null, onRejected: () => void) {
  const q = query.trim();
  const key = `${q}\0${device}`;
  const [paged, setPaged] = useState({ key, offset: 0 });
  const offset = paged.key === key ? paged.offset : 0;
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!q) {
      setOutcome(null);
      return;
    }
    setOutcome((o) => (o?.error ? null : o));
    let stale = false;
    const id = setTimeout(() => {
      api.search(token, q, device, offset).then(
        (hits) => !stale && setOutcome({ q, device, offset, hits }),
        (e) => !stale && (e instanceof TokenRejected ? onRejected() : setOutcome({ q, device, offset, error: e.message })),
      );
    }, DEBOUNCE_MS);
    return () => {
      stale = true;
      clearTimeout(id);
    };
  }, [api, token, q, device, offset, onRejected, attempt]);
  const current = outcome && outcome.q === q && outcome.device === device && outcome.offset === offset ? outcome : null;
  return {
    active: !!q,
    pending: !!q && !current,
    hits: current?.hits ?? null,
    previous: (outcome?.device === device && outcome.hits) || null,
    more: !!current?.hits?.[0]?.more,
    offset,
    setPage: (n: number) => setPaged({ key, offset: Math.max(0, n) * SEARCH_PAGE }),
    error: current?.error ?? null,
    retry: () => setAttempt((n) => n + 1),
  };
}
