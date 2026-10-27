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
 * An outcome is stored with the query it answers, so a reply to an older input is never shown as the answer to the current one, and `pending` covers the debounce and the request.
 * `previous` is the last answer, even an empty one, while the next one is pending, for showing it dimmed. It is dropped when the field is cleared or the device changes.
 * A change of `epoch` (the loaded library) sends the search again.
 */
export function useSearch(api: Api, token: string, query: string, device: string | null, epoch: unknown, onRejected: () => void) {
  const q = query.trim();
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
      api.search(token, q, device).then(
        (hits) => !stale && setOutcome({ q, device, hits }),
        (e) => !stale && (e instanceof TokenRejected ? onRejected() : setOutcome({ q, device, error: e.message })),
      );
    }, DEBOUNCE_MS);
    return () => {
      stale = true;
      clearTimeout(id);
    };
  }, [api, token, q, device, epoch, onRejected, attempt]);
  const current = outcome && outcome.q === q && outcome.device === device ? outcome : null;
  return {
    active: !!q,
    pending: !!q && !current,
    hits: current?.hits ?? null,
    previous: (outcome?.device === device && outcome.hits) || null,
    more: !!current?.hits?.[0]?.more,
    error: current?.error ?? null,
    retry: () => setAttempt((n) => n + 1),
  };
}
