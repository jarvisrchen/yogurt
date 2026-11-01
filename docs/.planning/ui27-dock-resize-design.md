# UI-27: resize the meeting dock and change its text size

Design, not yet implemented.
Ticket: `docs/TODO.md` UI-27.
Mocks: [../.lavish/ui27-dock-resize-design.html](../.lavish/ui27-dock-resize-design.html), with an interactive prototype at the top.

The dock (`web/src/components/TranscriptDock.tsx`) is a fixed 330 px panel with hardcoded text sizes.
On a wide screen it wastes room, and on a long call 14 px text is hard to read at a glance.
This adds two controls: drag the panel's left edge to resize it, and an Aa button that picks one of four text sizes.
Both persist, and both apply on the live page and the post-meeting page, since both render `TranscriptDock`.

## Resize

The panel's left border is the handle.
An 8 px strip straddles the border with a `col-resize` cursor.
A 2 px blueberry line shows on hover, focus, and drag.
Double-click resets the width to 330 px.

Width clamps to 320 px minimum, 600 px maximum, and never more than the window width minus 400 px, so the notes always keep some room.
320 px is the narrowest width where the recap footer still fits "Updated 1 min ago", Update now, and Pause on one row, and the header fits the Recap / Transcript toggle, the connection status, and the Aa button.
If the window shrinks below what a saved width allows, the panel clamps at render time and the saved value stays, so a wide width comes back on a wide window.

The edge tabs already take `right: 330px` when open.
They take the live width instead, and drop their 340 ms slide transition while a drag is in progress so they track the pointer.
The drag listens on `window` for `pointermove` and `pointerup` and disables text selection on the body for its duration.

The handle is a focusable `role="separator"` with `aria-orientation="vertical"` and `aria-valuenow/min/max`.
Left and Right arrows change the width by 20 px, and Home and End jump to the minimum and maximum.

## The notes column now moves out of the way

Today the dock overlays the notes, and the layout comment in `web/src/routes/Meeting.tsx` records that the notes column does not reflow.
That rule works because 330 px fits in the margin beside a 660 px column on a wide window.
At 600 px the panel would cover about 200 px of the notes, including the end of every line the user is typing.
Mock 4 shows it.

So when the dock is open, the page wrapper's right padding grows just enough to clear the panel and the 28 px edge tabs that sit left of it:

```css
--dock-clear: calc(var(--dock-w) + 28px);
padding-right: clamp(28px, calc(2 * var(--dock-clear) + 660px - 100vw), var(--dock-clear));
```

With C the dock width plus 28 px and V the window width, the centered 660 px column ends at (V - P + 660) / 2, which must not pass V - C.
That gives P = 2C + 660 - V, floored at today's 28 px gutter and capped at C, past which the column narrows instead of shifting.
At 1440 px with the default 330 px dock, P stays 28 px, so nothing moves for anyone who never resizes.
At 1440 px with a 600 px dock, the column shifts left and keeps 660 px.
At 1180 px with a 600 px dock, it narrows to 552 px.
The padding animates with the dock's existing 340 ms slide easing on open and close, and follows the pointer without a transition during a drag.

`TranscriptDock` sets `--dock-w` on `document.documentElement` while open and removes it on close, which keeps the wrappers in `Meeting.tsx` and `MeetingPost.tsx` free of new props.
Both wrappers swap `pr-7` for the padding rule above, and the `Meeting.tsx` layout comment is updated.

## Text size

An Aa button (lucide `ALargeSmall`) sits at the right end of the dock header, after the connection status.
It opens a popover with a four-step segmented control in the style of the Enhanced / My notes toggle.

| Step | Text | Timestamp |
| --- | --- | --- |
| Small | 13 px | 11 px |
| Default | 14 px | 12 px |
| Large | 16 px | 14 px |
| Extra large | 18 px | 15 px |

A pick applies at once and the popover stays open, so the user can compare steps.
Escape, a click outside, or a second click on Aa closes it.

Only the list text scales: transcript lines and recap bullets.
The header, footer, edge tabs, and empty and error messages keep their size.
The list container sets `font-size` to the step's pixel value, and the rows switch from fixed pixels to `em`: text at 1em, timestamps at 0.857em, the speaker column at 3.43em.
That keeps every row aligned at every step.
`TranscriptLine` is also used outside the dock, so it takes its sizes from the parent instead of hardcoding `text-[14px]`, and its other callers set 14 px on their container.

## Persistence

Two `localStorage` keys, `yogurt-dock-width` and `yogurt-dock-text`, the same store `web/src/lib/theme.ts` uses for the theme preference.
These are per-browser layout preferences, so they do not belong in server settings and do not sync to Yogurt Cloud.
Reads and writes go through try/catch and fall back to 330 px and 14 px.
The width writes once on drag end, not on every pointer move.

## Tests

Unit tests for the clamp (minimum, maximum, small-window cap, reset) and the padding formula at the three widths above.
A component test that dragging the handle and pressing arrow keys change `aria-valuenow`, and that a size pick changes the list's `font-size` and survives a remount.
Playwright screenshots of mocks 1, 3, and 5 in light and dark for the PR.

## Out of scope

Resizing the dock from its other edge or collapsing it to an icon rail.
Scaling the notes editor's text, which browser zoom already covers.
Keyboard shortcuts for text size, since `⌘+` and `⌘-` belong to browser zoom.
