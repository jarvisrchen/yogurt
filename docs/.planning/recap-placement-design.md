# Recap placement

Written 2026-11-01.
Ticket: `docs/TODO.md` MTG-28.
Builds on [live-recap-design.md](live-recap-design.md), which shipped the recap in the transcript dock (MTG-22).

The recap lives in the dock, which starts closed and overlays the notes on a narrow window.
To glance at the recap the user opens a panel, reads, and closes it again, which is the catch-up friction the recap was meant to remove.
MTG-22 considered a card above the notes and dropped it to keep the notes column still.
This brings that card back as a choice: a setting picks whether the recap shows in the dock, as today, or in the main column above the notes.

Mocks, light and dark: [.lavish/recap-placement-mocks.html](../.lavish/recap-placement-mocks.html).

## What the user sees

### The setting

General settings keeps "Show a live recap of the discussion while recording" and gains a picker under it, labeled "Show the recap", with two tiles side by side:

- In the transcript panel (the default, today's behavior)
- Above my notes

Each tile is a small sketch of the meeting page, the header bars and the notes card, with the recap drawn in blueberry where it would land: a panel on the right edge, or a block above the notes.
It follows the Notifications pane in macOS System Settings, which pictures Desktop, Notification Center, and Lock Screen, because where something appears is easier to see than to read.
The selected tile carries a 2px blueberry ring and a semibold label.
The picker is a `role="radiogroup"` of native radios, visually hidden and labelled by their tiles, so arrow keys move between the two and a screen reader announces "Show the recap, In the transcript panel, 1 of 2".
While the recap checkbox is off the tiles dim to 50% and the radios are disabled, since there is nothing to place.
The sketches are plain markup with the app's tokens, so they follow light and dark with no images.

The privacy line under the checkbox stays as it is, since placement changes nothing about what is sent.

### The card on the live page

With "Above my notes", the live page (`web/src/routes/Meeting.tsx`) shows a Recap card between the header and the notes card, under any banner (silence stop, start error, STT error).
It uses the notes card's frame: `rounded-card bg-card` with the `LINE` border, at the same 660px column width.

The card has a header row and a list.
The header reads `RECAP · Updated 1 min ago` on the left, in the settings label style, and holds Update now, Pause, and a collapse chevron on the right.
The controls move from the dock's footer into this header, so the card spends one row on chrome instead of two.
The list renders the bullets exactly as the dock does: time on the left, one line of fact, append-only, newest at the bottom, sticky to the bottom unless the user has scrolled up.

The list has a fixed height of four bullet rows, 148px, from the moment recording starts.
Four is the most one update can add, so an update of one-line bullets is always fully in view.
The fixed height is what keeps the notes still: the card never grows as bullets arrive, so the editor never moves under the user's cursor.
Before the first update the reserved space holds the waiting line, "The recap starts after about 3 minutes of conversation."
Older bullets scroll inside the card, under a 20px fade at the list's top edge that shows there is more above.

Every state the dock shows carries over unchanged: the shimmer row while updating, the paused line with Resume in place of Update now and Pause, and the strawberry-soft error under the bullets.

The card shows while recording, and while stopped if the meeting has bullets or its recap is paused, so Resume stays reachable.
A meeting that has never recorded, or stopped before the first update, has no card.

The dock in this mode is the Transcript-only dock, exactly as with the recap setting off: one edge tab, no Recap view, no unseen dot.

### Collapsing

The chevron collapses the card to its header row, so a user who wants the notes column back can have it without leaving the meeting.
While collapsed, new bullets show as a blueberry dot and `2 new` in the header, the card's version of the dock's unseen dot.
Expanding clears it.
Collapsing is a click by the user, so the notes moving up then is expected.
The collapsed state is per meeting and resets on the next meeting, held in component state.

### Jumping to the transcript

Clicking a bullet opens the dock on Transcript and scrolls to that moment, through the same `yogurt:transcript:scrollTo` event the post-meeting timestamp links dispatch.
Today that event only scrolls a dock that is already open, and `MeetingPost` opens the dock itself before dispatching.
The dock's listener (`web/src/components/TranscriptDock.tsx:182`) will open the dock itself, so every caller gets the same behavior and the live page needs no open-state wiring of its own.
`MeetingPost`'s own `setTranscriptOpen(true)` then becomes redundant and goes away.

### After the meeting

The post-meeting page (`web/src/routes/MeetingPost.tsx`) shows the same card above the Enhanced and My notes tabs, because the recap belongs to the whole meeting.
In final mode the header reads `RECAP · Final, the summary replaces it when ready` and has no Update now or Pause, matching the dock's final footer.
When the server sends the empty `live_recap` frame after Enhance saves the summary, the card collapses its height over 200ms and goes away, so the summary slides up into its place.
The `dockRecap` navigation state is ignored in this mode, since there is no Recap view in the dock to reopen.

### Switching mid-meeting

Both placements read the same server state, so changing the setting during a meeting moves the bullets on the next settings fetch with nothing lost.
Switching to the dock does not light the unseen dot, because the dock counts the bullets it finds on load as seen, as it does today.
A paused recap stays paused in either place.

## How it is built

### The setting

`general.live_recap_placement` is a new key in the key-value settings (`crates/yogurt-db/src/settings.rs`), so it needs no migration.
In Rust it is an enum, `RecapPlacement { Dock, Main }`, serialized as `"dock"` and `"main"`.
A missing or unrecognized stored value reads as `Dock`, so upgrading users see no change.
`SettingsPatch` takes it as `Option<RecapPlacement>`, so a PATCH with any other string fails deserialization and is rejected before it reaches the database.
`yogurt ctl settings` prints it beside `live_recap`.
The recap task never reads it: placement is presentation only, and what is sent, when, and to whom is unchanged.

### The frontend

`RecapView` in `TranscriptDock.tsx` splits into two pieces, both moved to a new `web/src/components/Recap.tsx`:

- `RecapBullets`: the list, with the bullets, the shimmer, the waiting, paused, and error lines.
- `RecapStatus`: the status text and the Update now, Pause, and Resume buttons.

The dock composes them as today, the list above and the status in its footer.
A new `RecapCard` in the same file composes them as the card, the status in the header beside the chevron.
`useStickyBottom` moves with them, since both containers need it.
`RecapCard` calls `useLiveRecap`, which already shares one meeting socket across subscribers since MTG-25, and in this mode it is the only subscriber because the dock gets no `recap` prop.

`Meeting.tsx` and `MeetingPost.tsx` each read the placement next to `live_recap` and pass `recap` to either the dock or the card, never both.

The post page's swatch legend was absolutely positioned at the top of the column, which put it above the card.
It moves into the document tab row, at its right end, so it labels the document in either placement.

## Testing

`yogurt-db` tests the new key: missing reads as `dock`, `main` round-trips, an unknown stored string reads as `dock`.
A server test checks that a PATCH with an unknown placement is rejected and leaves the stored value alone.
Vitest covers the card: the reserved height before the first bullet, bullets appending without changing the card's height, the collapsed `N new` count, a bullet click opening the dock on Transcript at the right line, Pause and Resume from the header, the final-mode header, and the card leaving on an empty frame.
It also covers the dock with placement `main`: Transcript only, no Recap tab, no dot.
The existing dock tests keep passing after the split, which is the check that `RecapBullets` and `RecapStatus` render what `RecapView` did.
Playwright screenshots the card in light and dark for the PR.
A real meeting with `just dev-bg` checks the fixed height while typing, since that is a feel only a live recording shows.

## Docs

- `docs/FEATURES.md`: the Live recap row names the placement setting and the card.
- `docs/AI-INTEGRATION.md`: nothing, since no route or frame changes.

## Decided

- Two placements, the dock and a card above the notes, picked in General settings with two picture tiles.
- The dock stays the default, so nobody's meeting page changes until they choose.
- The card's list has a fixed height of four rows, reserved from the start of recording, so the notes never move while the user types.
- The card's controls live in its header.
- Collapse is per meeting, held in component state.
- On the post page the card sits above the document tabs and collapses away when the summary lands.
- A bullet click opens the dock on Transcript, and the dock's scroll listener does the opening for every caller.
- Placement is presentation only, so the recap task, its routes, and its frames do not change.

## Open

- Whether "Above my notes" should become the default once it has run in real meetings.
- Whether collapse should be remembered across meetings, which would move it into `localStorage`.
