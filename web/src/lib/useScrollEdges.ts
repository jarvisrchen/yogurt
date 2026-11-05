import { useEffect, useState } from "react";

/**
 * Whether a scroller sits at its top and at its bottom. Content that fits sits at both.
 * Pass the element itself, from a ref callback, so a remounted element is watched again.
 * It rechecks on a scroll, when the element or a child resizes, and when its content changes, never on a plain render, so a drag that re-renders each frame does not force a layout each frame.
 */
export function useScrollEdges(el: HTMLElement | null) {
  const [edges, setEdges] = useState({ top: true, bottom: true });
  useEffect(() => {
    if (!el) return;
    // Every streamed word and scrolled pixel calls this, so it sets state only when an edge flips.
    let seen = "";
    const check = () => {
      const top = el.scrollTop <= 1;
      const bottom = el.scrollHeight - el.scrollTop - el.clientHeight <= 1;
      if (seen === `${top}${bottom}`) return;
      seen = `${top}${bottom}`;
      setEdges({ top, bottom });
    };
    const ro = new ResizeObserver(check);
    const watch = () => {
      ro.disconnect();
      ro.observe(el);
      for (const child of el.children) ro.observe(child);
      check();
    };
    const mo = new MutationObserver(watch);
    mo.observe(el, { childList: true, subtree: true, characterData: true });
    el.addEventListener("scroll", check, { passive: true });
    watch();
    return () => {
      ro.disconnect();
      mo.disconnect();
      el.removeEventListener("scroll", check);
    };
  }, [el]);
  return edges;
}

/**
 * The touch-action that leaves the browser the scroll a finger can still do, and hands a drag the rest.
 * `up` and `down` say which finger directions the drag takes. The browser must know before the finger lands, so this goes on the element as a class.
 */
export const dragTouch = (up: boolean, down: boolean) => (up && down ? "touch-none" : up ? "touch-pan-up" : down ? "touch-pan-down" : "");
