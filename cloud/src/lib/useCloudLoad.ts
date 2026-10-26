import { useEffect, useState } from "react";
import { TokenRejected } from "./api";

/**
 * Runs `load` again when `deps` change or on `retry`. A reply from a superseded run is dropped, so a slow earlier request never overwrites a newer one.
 * `data` is what the last run for the current `deps` returned, so a retry keeps it on screen while `loading`, and new `deps` start from null.
 */
export function useCloudLoad<T>(load: () => Promise<T>, onRejected: () => void, deps: unknown[]) {
  const [loaded, setLoaded] = useState<{ deps: unknown[]; data: T } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let stale = false;
    setError(null);
    setLoading(true);
    load().then(
      (data) => {
        if (stale) return;
        setLoaded({ deps, data });
        setLoading(false);
      },
      (e) => {
        if (stale) return;
        setLoading(false);
        if (e instanceof TokenRejected) onRejected();
        else setError(e.message);
      },
    );
    return () => {
      stale = true;
    };
    // `load` closes over `deps`, so the caller lists what it reads.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, onRejected, attempt]);
  const current = loaded && loaded.deps.length === deps.length && loaded.deps.every((d, i) => Object.is(d, deps[i]));
  return { data: current ? loaded.data : null, error, loading, retry: () => setAttempt((n) => n + 1) };
}
