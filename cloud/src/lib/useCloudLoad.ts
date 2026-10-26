import { useEffect, useRef, useState } from "react";
import { TokenRejected } from "./api";

/** Runs `load` again when `deps` change, on `retry`, or on `refresh`, which keeps the current data on screen until the new data lands, and keeps it when the refresh fails. `done` runs once that run finishes either way. A reply from a superseded run is dropped, so a slow earlier request never overwrites a newer one. */
export function useCloudLoad<T>(load: () => Promise<T>, onRejected: () => void, deps: unknown[]) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);
  const quiet = useRef(false);
  const after = useRef<(() => void) | null>(null);
  useEffect(() => {
    let stale = false;
    const silent = quiet.current;
    const done = after.current;
    quiet.current = false;
    after.current = null;
    if (!silent) {
      setError(null);
      setData(null);
    }
    load().then(
      (d) => !stale && (setError(null), setData(d), done?.()),
      (e) => !stale && (e instanceof TokenRejected ? onRejected() : (!silent && setError(e.message), done?.())),
    );
    return () => {
      stale = true;
    };
    // `load` closes over `deps`, so the caller lists what it reads.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, onRejected, attempt]);
  return {
    data,
    error,
    retry: () => setAttempt((n) => n + 1),
    refresh: (done?: () => void) => {
      quiet.current = true;
      after.current = done ?? null;
      setAttempt((n) => n + 1);
    },
  };
}
