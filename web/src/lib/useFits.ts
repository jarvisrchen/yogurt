import { useEffect, useState } from "react";

/**
 * Whether an element's content fits without scrolling.
 * Pass the element itself, from a ref callback, so a remounted element is watched again.
 * It rechecks when the element or a child resizes and when its content changes, never on a plain render, so a drag that re-renders each frame does not force a layout each frame.
 */
export function useFits(el: HTMLElement | null) {
  const [fits, setFits] = useState(true);
  useEffect(() => {
    if (!el) return;
    const check = () => setFits(el.scrollHeight <= el.clientHeight + 1);
    const ro = new ResizeObserver(check);
    const watch = () => {
      ro.disconnect();
      ro.observe(el);
      for (const child of el.children) ro.observe(child);
      check();
    };
    const mo = new MutationObserver(watch);
    mo.observe(el, { childList: true, subtree: true, characterData: true });
    watch();
    return () => {
      ro.disconnect();
      mo.disconnect();
    };
  }, [el]);
  return fits;
}
