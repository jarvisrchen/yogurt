import { useEffect, useState } from "react";
import { TokenRejected, type Api, type SearchHit } from "./api";

const DEBOUNCE_MS = 250;

/** Hits for `query` from the server, or null while the query is blank or nothing has come back yet. A reply for an older query or push is dropped, and the previous hits stay until the new ones land. */
export function useSearch(api: Api, token: string, atPush: number | null, query: string, onRejected: () => void) {
  const [state, setState] = useState<{ hits: SearchHit[] | null; error: string | null }>({ hits: null, error: null });
  const q = query.trim();
  useEffect(() => {
    if (!q) {
      setState({ hits: null, error: null });
      return;
    }
    let stale = false;
    const id = setTimeout(() => {
      api.search(token, q, atPush ?? undefined).then(
        (hits) => !stale && setState({ hits, error: null }),
        (e) => !stale && (e instanceof TokenRejected ? onRejected() : setState({ hits: null, error: e.message })),
      );
    }, DEBOUNCE_MS);
    return () => {
      stale = true;
      clearTimeout(id);
    };
  }, [api, token, atPush, q, onRejected]);
  return { ...state, active: !!q };
}
