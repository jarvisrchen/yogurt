import { useState } from "react";

// The library view a meeting page returns to, with its scroll, kept per tab so a reload of the meeting still finds it.
const KEY = "yogurt.library.return";

interface Spot {
  url: string;
  y: number;
}

export function rememberLibrary(spot: Spot): void {
  try {
    sessionStorage.setItem(KEY, JSON.stringify(spot));
  } catch {
    // Blocked storage just sends the Library link to All meetings.
  }
}

export function librarySpot(): Spot | null {
  try {
    const s = JSON.parse(sessionStorage.getItem(KEY) ?? "null");
    return typeof s?.url === "string" && typeof s.y === "number" ? s : null;
  } catch {
    return null;
  }
}

export const libraryUrl = (): string => librarySpot()?.url ?? "/";

/** Navigation options for a return to `libraryUrl()`: the Library restores the scroll itself once its rows are in. */
export const RETURN = { state: { libraryReturn: true }, preventScrollReset: true } as const;

/** Props for a page's Library link, read once since the remembered view cannot change while the page is open. */
export function useLibraryBack() {
  const [to] = useState(libraryUrl);
  return { to, ...RETURN };
}

/** The search to carry to another library view, so switching labels keeps it. */
export function keepSearch(search: string): string {
  const q = new URLSearchParams(search).get("q");
  return q ? `?${new URLSearchParams({ q })}` : "";
}
