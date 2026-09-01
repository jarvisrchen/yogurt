/**
 * UI-6: dark mode.
 *
 * The preference lives in localStorage rather than server settings so the
 * inline script in `index.html` can stamp `data-theme` before first paint
 * (no light flash on reload). `index.css` overrides the `@theme` color
 * tokens under `:root[data-theme="dark"]`, so every `bg-paper` / `text-ink`
 * utility and every `var(--color-*)` inline style follows for free.
 *
 * Keep `resolve()` in sync with the inline script in `web/index.html`.
 */
export type ThemePref = "system" | "light" | "dark";

export const THEME_KEY = "yogurt-theme";

const DARK_MQ = "(prefers-color-scheme: dark)";

export function getThemePref(): ThemePref {
  try {
    const v = localStorage.getItem(THEME_KEY);
    return v === "light" || v === "dark" ? v : "system";
  } catch {
    return "system";
  }
}

export function resolveTheme(pref: ThemePref): "light" | "dark" {
  if (pref !== "system") return pref;
  return typeof matchMedia === "function" && matchMedia(DARK_MQ).matches
    ? "dark"
    : "light";
}

export function applyTheme(pref: ThemePref = getThemePref()): void {
  document.documentElement.dataset.theme = resolveTheme(pref);
}

export function setThemePref(pref: ThemePref): void {
  try {
    if (pref === "system") localStorage.removeItem(THEME_KEY);
    else localStorage.setItem(THEME_KEY, pref);
  } catch {
    // Private mode / storage disabled: still apply for this page load.
  }
  applyTheme(pref);
}

/** Apply once and re-apply when the OS scheme flips while on "system". */
export function initTheme(): void {
  applyTheme();
  if (typeof matchMedia !== "function") return;
  matchMedia(DARK_MQ).addEventListener("change", () => {
    if (getThemePref() === "system") applyTheme("system");
  });
}
