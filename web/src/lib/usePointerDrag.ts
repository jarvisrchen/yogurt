import { useEffect, useRef, useState } from "react";

/**
 * A resize handle's drag: `start` follows the pointer on the window and reports how far it moved from the press,
 * then `onDone` once it is released. While dragging, `rootClass` on <html> holds the cursor and blocks text selection.
 */
export function usePointerDrag(rootClass: string) {
  const [dragging, setDragging] = useState(false);
  // Detaches a drag's window listeners if the handle unmounts mid-drag.
  const endRef = useRef<(() => void) | null>(null);
  useEffect(() => () => endRef.current?.(), []);
  useEffect(() => {
    document.documentElement.classList.toggle(rootClass, dragging);
    return () => document.documentElement.classList.remove(rootClass);
  }, [dragging, rootClass]);

  function start(e: React.PointerEvent, onMove: (dx: number, dy: number) => void, onDone: (moved: boolean) => void) {
    // A second pointer while one drags would stack listeners and fight over the size.
    if (e.button > 0 || endRef.current) return;
    e.preventDefault();
    const { clientX, clientY } = e;
    let moved = false;
    setDragging(true);
    const move = (ev: PointerEvent) => {
      // The release can land outside the page (another app, Cmd-Tab), so a move with no button held ends the drag.
      if (ev.buttons === 0) return end();
      moved = true;
      onMove(ev.clientX - clientX, ev.clientY - clientY);
    };
    const end = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", end);
      window.removeEventListener("pointercancel", end);
      endRef.current = null;
      setDragging(false);
      onDone(moved);
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", end);
    window.addEventListener("pointercancel", end);
    endRef.current = end;
  }

  return { dragging, start };
}
