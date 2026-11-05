import { useEffect, useRef, useState } from "react";

// Release speed is read over this window, so one coalesced or noisy event does not decide a flick.
const SPEED_WINDOW_MS = 60;
// A finger held still this long before letting go is no flick.
const STILL_MS = 80;

/**
 * A resize handle's drag: `start` follows the pointer on the window and reports how far it moved from the press,
 * then `onDone` once it is released, with whether it was cancelled and its vertical speed at release in px/ms. While dragging, `rootClass` on <html> holds the cursor and blocks text selection.
 */
export function usePointerDrag(rootClass: string) {
  const [dragging, setDragging] = useState(false);
  // Detaches a drag's window listeners if the handle unmounts mid-drag.
  const endRef = useRef<((cancelled: boolean) => void) | null>(null);
  useEffect(() => () => endRef.current?.(true), []);
  useEffect(() => {
    document.documentElement.classList.toggle(rootClass, dragging);
    return () => document.documentElement.classList.remove(rootClass);
  }, [dragging, rootClass]);

  function start(e: React.PointerEvent, onMove: (dx: number, dy: number) => void, onDone: (moved: boolean, cancelled: boolean, speedY: number) => void) {
    // A second pointer while one drags would stack listeners and fight over the size.
    if (e.button > 0 || endRef.current) return;
    e.preventDefault();
    const { clientX, clientY } = e;
    let moved = false;
    const samples = [{ y: clientY, t: performance.now() }];
    setDragging(true);
    const move = (ev: PointerEvent) => {
      // The release can land outside the page (another app, Cmd-Tab), so a move with no button held ends the drag.
      if (ev.buttons === 0) return finish(false);
      moved = true;
      const t = performance.now();
      samples.push({ y: ev.clientY, t });
      while (samples.length > 2 && t - samples[1].t > SPEED_WINDOW_MS) samples.shift();
      onMove(ev.clientX - clientX, ev.clientY - clientY);
    };
    const speedY = () => {
      const first = samples[0];
      const last = samples[samples.length - 1];
      if (performance.now() - last.t > STILL_MS) return 0;
      return (last.y - first.y) / Math.max(last.t - first.t, 1);
    };
    const finish = (cancelled: boolean) => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", cancel);
      endRef.current = null;
      setDragging(false);
      onDone(moved, cancelled, cancelled ? 0 : speedY());
    };
    const up = () => finish(false);
    const cancel = () => finish(true);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", cancel);
    endRef.current = finish;
  }

  return { dragging, start };
}
