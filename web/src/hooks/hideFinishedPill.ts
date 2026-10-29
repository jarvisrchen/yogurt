/** How long a finished import or push pill stays up once the page is visible. */
export const DONE_PILL_MS = 5000;

/**
 * Calls `hide` DONE_PILL_MS after the page is next visible, so a run that ends in a background tab still shows its result.
 * Does nothing if `stillShown` is false by then, because a newer state or the user already replaced the pill.
 */
export function hideFinishedPill(stillShown: () => boolean, hide: () => void) {
  const start = () => {
    if (document.hidden) {
      document.addEventListener("visibilitychange", start, { once: true });
      return;
    }
    window.setTimeout(() => {
      if (stillShown()) hide();
    }, DONE_PILL_MS);
  };
  start();
}
