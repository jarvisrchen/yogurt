export interface Status {
  owner_email_set: boolean;
  is_owner: boolean;
  passcode_set: boolean;
  /** Digits in the PIN, or null before one is set and for a PIN not yet backfilled by an unlock. */
  passcode_length: number | null;
  locked_until: string | null;
}

/** A PIN is 4 or 6 digits. The stored length only picks how many boxes to draw, so the server stays the judge of a PIN. */
export const validPin = (pin: string): boolean => /^(\d{4}|\d{6})$/.test(pin);

/** Boxes to draw: the stored length while what is typed still fits in it, else 6. */
export const pinBoxes = (stored: number | null, typed: string): number => (stored && typed.length <= stored ? stored : 6);

export type View = "library" | "settings" | "change-pin" | "history";
export type Screen = "signin" | "not-owner" | "choose-pin" | "unlock" | View;

/** `status` is null until yogurt_status has answered, and is only fetched once there is a session. */
export function screenFor(o: { signedIn: boolean; status: Status | null; token: string | null; view: View }): Screen | "loading" {
  if (!o.signedIn) return "signin";
  if (!o.status) return "loading";
  if (!o.status.is_owner) return "not-owner";
  if (!o.status.passcode_set) return "choose-pin";
  if (!o.token) return "unlock";
  return o.view;
}

/** Whole minutes left on a lockout, or 0 when not locked. Rounds to the nearest minute so a few seconds of clock skew still reads "15 minutes". */
export function lockoutMinutes(lockedUntil: string | null, now = Date.now()): number {
  const ms = lockedUntil ? Date.parse(lockedUntil) - now : 0;
  return ms > 0 ? Math.max(1, Math.round(ms / 60_000)) : 0;
}

export function formatWait(minutes: number): string {
  if (minutes >= 120) return `${Math.round(minutes / 60)} hours`;
  if (minutes >= 60) return minutes === 60 ? "1 hour" : `${minutes} minutes`;
  return minutes === 1 ? "1 minute" : `${minutes} minutes`;
}

export function browserLabel(ua = navigator.userAgent): string {
  const os = /iPhone/.test(ua) ? "iPhone" : /iPad/.test(ua) ? "iPad" : /Android/.test(ua) ? "Android" : /Mac OS X/.test(ua) ? "Mac" : /Windows/.test(ua) ? "Windows" : /Linux/.test(ua) ? "Linux" : "";
  const b = /Edg\//.test(ua) ? "Edge" : /Firefox\//.test(ua) ? "Firefox" : /Chrome\/|CriOS/.test(ua) ? "Chrome" : /Safari\//.test(ua) ? "Safari" : "Browser";
  return os ? `${b} on ${os}` : b;
}
