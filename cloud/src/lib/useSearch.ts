import { useEffect, useState } from "react";
import { TokenRejected, type Api, type SearchHit } from "./api";

const DEBOUNCE_MS = 250;

interface Outcome {
  q: string;
  device: string | null;
  hits?: SearchHit[];
  error?: string;
}

/**
 * Server search for `query`, optionally within one device.
 * Hits and errors are stored with the query they answer, so a reply to an older input is never shown as the answer to the current one, and `pending` covers the debounce and the request.
 * `previous` is the last answer while the next one is pending, for showing it dimmed.
 */
export function useSearch(api: Api, token: string, query: string, device: string | null, onRejected: () => void) {
  const q = query.trim();
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (!q) return;
    let stale = false;
    const id = setTimeout(() => {
      api.search(token, q, device).then(
        (hits) => !stale && setOutcome({ q, device, hits }),
        (e) => !stale && (e instanceof TokenRejected ? onRejected() : setOutcome({ q, device, error: e.message })),
      );
    }, DEBOUNCE_MS);
    return () => {
      stale = true;
      clearTimeout(id);
    };
  }, [api, token, q, device, onRejected, attempt]);
  const current = outcome && outcome.q === q && outcome.device === device ? outcome : null;
  return {
    active: !!q,
    pending: !!q && !current,
    hits: current?.hits ?? null,
    previous: outcome?.hits?.length ? outcome.hits : null,
    more: !!current?.hits?.[0]?.more,
    error: current?.error ?? null,
    retry: () => {
      setOutcome(null);
      setAttempt((n) => n + 1);
    },
  };
}
