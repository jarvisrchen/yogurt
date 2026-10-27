import { useCallback, useEffect, useState } from "react";
import { PAGE_SIZES, parsePage } from "@web/components/library/pagination";
import type { View } from "./route";

/** Where the signed-in webapp is. `meeting` is `<device_id>/<meeting_id>`, the key of a library entry. `size` is null until the URL names one. */
export interface Loc {
  view: View;
  asOf: number | null;
  meeting: string | null;
  page: number;
  size: number | null;
}

export const HOME: Loc = { view: "library", asOf: null, meeting: null, page: 1, size: null };

export function parse(hash: string): Loc {
  let parts: string[];
  try {
    parts = hash.replace(/^#\/?/, "").split("/").filter(Boolean).map(decodeURIComponent);
  } catch {
    return HOME;
  }
  const [a, b] = parts;
  if (a === "settings") return { ...HOME, view: b === "pin" ? "change-pin" : "settings" };
  if (a === "history") return { ...HOME, view: "history" };
  let asOf: number | null = null;
  let rest = parts;
  if (a === "at") {
    asOf = Number(b);
    if (!Number.isSafeInteger(asOf) || asOf < 1) return HOME;
    rest = parts.slice(2);
  }
  const loc: Loc = { ...HOME, asOf };
  for (let i = 0; i < rest.length; ) {
    if (rest[i] === "p") loc.page = parsePage(rest[i + 1] ?? null);
    else if (rest[i] === "n") loc.size = PAGE_SIZES.find((n) => String(n) === rest[i + 1]) ?? null;
    else if (rest[i] === "m" && rest[i + 1] && rest[i + 2]) return { ...loc, meeting: `${rest[i + 1]}/${rest[i + 2]}` };
    else break;
    i += 2;
  }
  return loc;
}

export function format(l: Loc): string {
  if (l.view === "settings") return "#/settings";
  if (l.view === "change-pin") return "#/settings/pin";
  if (l.view === "history") return "#/history";
  const at = l.asOf ? `/at/${l.asOf}` : "";
  const paging = `${l.page > 1 ? `/p/${l.page}` : ""}${l.size ? `/n/${l.size}` : ""}`;
  // A meeting id may hold a slash, so only the first one separates the device.
  const i = l.meeting?.indexOf("/") ?? -1;
  const m = l.meeting ? `/m/${encodeURIComponent(l.meeting.slice(0, i))}/${encodeURIComponent(l.meeting.slice(i + 1))}` : "";
  return at + paging + m ? `#${at}${paging}${m}` : "";
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

  /** Swaps the current entry for `to`, keeping the back-stack marker. For a location that should not be a stop of its own. */
  const replace = useCallback((to: Loc) => {
    history.replaceState(history.state, "", url(format(to)));
    setLoc(to);
  }, []);

  const reset = useCallback(() => {
    history.replaceState(null, "", url(""));
    setLoc(HOME);
  }, []);

  return { loc, go, replace, back, reset };
}
