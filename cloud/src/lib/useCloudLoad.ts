import { useEffect, useState } from "react";
import { TokenRejected } from "./api";

/** Runs `load` again when `deps` change or on `retry`. A reply from a superseded run is dropped, so a slow earlier request never overwrites a newer one. */
export function useCloudLoad<T>(load: () => Promise<T>, onRejected: () => void, deps: unknown[]) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let stale = false;
    setError(null);
    setData(null);
    load().then(
      (d) => !stale && setData(d),
      (e) => !stale && (e instanceof TokenRejected ? onRejected() : setError(e.message)),
    );
    return () => {
      stale = true;
    };
    // `load` closes over `deps`, so the caller lists what it reads.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, onRejected, attempt]);
  return { data, error, retry: () => setAttempt((n) => n + 1) };
}
