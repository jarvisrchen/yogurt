export const TOKEN_KEY = "yogurt-cloud-browser";

/** The browser token and whether the user chose "Remember this browser", so a passcode change can unlock again the same way. */
export interface StoredToken {
  token: string;
  remember: boolean;
}

export function readToken(): StoredToken | null {
  try {
    const v = JSON.parse(localStorage.getItem(TOKEN_KEY) ?? "null");
    return typeof v?.token === "string" && v.token ? { token: v.token, remember: v.remember === true } : null;
  } catch {
    return null;
  }
}

export function saveToken(t: StoredToken): void {
  try {
    localStorage.setItem(TOKEN_KEY, JSON.stringify(t));
  } catch {
    // Storage blocked: the token lives for this page load only.
  }
}

export function clearToken(): void {
  try {
    localStorage.removeItem(TOKEN_KEY);
  } catch {
    // Nothing stored to clear.
  }
}
