import { useCallback, useEffect, useState } from "react";
import type { View } from "./route";

/** Where the signed-in webapp is. `meeting` is `<device_id>/<meeting_id>`, the key of a library entry. */
export interface Loc {
  view: View;
  asOf: number | null;
  meeting: string | null;
}

export const HOME: Loc = { view: "library", asOf: null, meeting: null };

export function parse(hash: string): Loc {
  let parts: string[];
  try {
    parts = hash.replace(/^#\/?/, "").split("/").filter(Boolean).map(decodeURIComponent);
  } catch {
    return HOME;
  }
  const [a, b, c, d, e] = parts;
  if (a === "settings") return { ...HOME, view: b === "pin" ? "change-pin" : "settings" };
  if (a === "history") return { ...HOME, view: "history" };
  let asOf: number | null = null;
  let rest = parts;
  if (a === "at") {
    asOf = Number(b);
    if (!Number.isSafeInteger(asOf) || asOf < 1) return HOME;
    rest = [c, d, e];
  }
  if (rest[0] === "m" && rest[1] && rest[2]) return { view: "library", asOf, meeting: `${rest[1]}/${rest[2]}` };
  return { ...HOME, asOf };
}

export function format(l: Loc): string {
  if (l.view === "settings") return "#/settings";
  if (l.view === "change-pin") return "#/settings/pin";
  if (l.view === "history") return "#/history";
  const at = l.asOf ? `/at/${l.asOf}` : "";
  // A meeting id may hold a slash, so only the first one separates the device.
  const i = l.meeting?.indexOf("/") ?? -1;
  const m = l.meeting ? `/m/${encodeURIComponent(l.meeting.slice(0, i))}/${encodeURIComponent(l.meeting.slice(i + 1))}` : "";
  return at + m ? `#${at}${m}` : "";
}

interface Mark {
  /** Entries this app pushed before this one, so history.back() stays inside it. */
  depth: number;
  from: string;
}
const mark = (): Mark | null => (history.state as { yg?: Mark } | null)?.yg ?? null;
const url = (hash: string) => location.pathname + location.search + hash;

/** The location lives in the URL hash, so reload and the back gesture work. Auth screens never touch it, which keeps a deep link through sign-in and unlock. */
export function useLoc() {
  const [loc, setLoc] = useState(() => parse(location.hash));
  useEffect(() => {
    const sync = () => setLoc(parse(location.hash));
    window.addEventListener("popstate", sync);
    return () => window.removeEventListener("popstate", sync);
  }, []);

  const go = useCallback((to: Loc) => {
    if (format(to) === format(parse(location.hash))) return;
    const m = mark();
    history.pushState({ yg: { depth: (m?.depth ?? 0) + 1, from: location.hash } }, "", url(format(to)));
    setLoc(to);
  }, []);

  /** A back link: step back when the previous entry is this app's own and shows the same screen, otherwise replace this entry with the target. */
  const back = useCallback((to: Loc) => {
    const m = mark();
    if (m && m.depth > 0 && parse(m.from).view === to.view) return history.back();
    history.replaceState(null, "", url(format(to)));
    setLoc(to);
  }, []);

  const reset = useCallback(() => {
    history.replaceState(null, "", url(""));
    setLoc(HOME);
  }, []);

  return { loc, go, back, reset };
}
