# Archiving meetings

Planned. Written 2026-10-27.
Tickets: `docs/TODO.md` MTG-21 (archive on the Mac), SYNC-21 (archive in the webapp), and SYNC-22 (each Mac's Starred and Labels in the webapp).

A Granola import brings in every meeting the user ever had, about 1,364 for the account this was designed against.
Most of them no longer matter, but they fill the Library, search, and Ask across meetings.
Deleting them is wrong, because a deleted import comes back on the next import and the user may still need an old meeting someday.

Archive hides a meeting everywhere it is listed or searched, and keeps all of it.
An Archive view lists only archived meetings, and its search box searches only them.
It behaves like Starred: a sidebar view with its own search, built on the same filter.

Mocks, light and dark: [.lavish/archive-mocks.html](../.lavish/archive-mocks.html).

## Decisions

| Question | Decision |
| --- | --- |
| Archive or delete | Archive is a flag. The meeting keeps its notes, transcript, chat, labels, star, and Granola ID. |
| Which meetings the date rule touches | All of them, recorded or imported. |
| Granola re-import | Skips archived meetings, because the import matches on the Granola ID, which archiving keeps. |
| How to archive many at once | Multi-select in the Library, Archive everything before a date, and Archive meetings in a label's menu (MTG-23). |
| The phone | Gets an Archive view, read-only like the rest of the webapp. Archiving happens only on the Mac. |
| Starred and labels on the phone | Starred is offered under All devices and each Mac. Labels and Archive appear once a Mac is picked, because each Mac has its own labels (changed by SYNC-23). |

## The Mac (MTG-21)

### Storage

`V016__meetings_archived.sql` adds `archived_at INTEGER` to `meetings`: unix milliseconds when it was archived, `NULL` when it is not.
It adds no index, because a scan of a few thousand rows is fast and SQLite cannot use a partial index behind a bound `:archived` anyway.

`Meeting` gains `archived_at: Option<DateTime<Utc>>` with `skip_serializing_if = "Option::is_none"`, the same attribute `source` uses.
A meeting that is not archived then hashes as it does today, so the migration pushes nothing to the Cloud.

Five places read meeting columns by position, and each gains the new column in the same slot:
`SELECT_ALL_WHERE_ID` and `row_to_meeting` (`crates/yogurt-db/src/meetings.rs:671-699`), the two column lists in `retrieve` (`:550-553`, `:566-569`), and the excerpt index `retrieve` reads (`:578`).

### Where archived meetings are hidden

One filter decides it.
`MeetingFilter` (`meetings.rs:87`) gains `archived: bool`, and `FILTER_SQL` (`:755`) gains:

```sql
AND ((m.archived_at IS NOT NULL) = :archived)
```

`list_page` and `search_page` both use `FILTER_SQL`, so All meetings, Starred, a label, and their searches hide archived meetings.
The Archive view and its search show only archived meetings.
The date-shaped search (`ON_DAY`) goes through the same clause.

Elsewhere:

- `retrieve` (`meetings.rs:534`), behind Ask across meetings, does not use `FILTER_SQL`.
  Both of its queries gain `archived_at IS NULL`.
- In `web/src/routes/Library.tsx`, `isFiltered` (`:86`) and `viewKey` (`:107`) gain `archivedOnly`.
  Otherwise the greeting would count the archive as the whole library, and moving between `/` and `/archive` would keep the old page.
- `labels::list_with_counts` (`crates/yogurt-db/src/labels.rs:45`) keeps counting archived meetings.
  The label delete confirmation reads that count (`web/src/components/library/SidebarLabelRow.tsx:208`), and deleting a label removes it from archived meetings too.
- `yogurt ctl meeting last`, `list`, and `search` call the same routes, so they skip archived meetings with no change.

Opening an archived meeting by its link still works, as do its chat, Enhance, Re-enhance, Copy markdown, Reveal in Finder, delete, its markdown file in the notes folder, and Cloud push.

### Routes

- `PATCH /api/meetings/{id}` takes `"archived": true | false`, beside `starred` (`crates/yogurt-server/src/api/meetings.rs:73`).
  It goes through `patch`, which bumps `updated_at` and rewrites the markdown export like any other edit.
  Archiving sets `archived_at` to now, and archiving an archived meeting keeps its first date.
- `GET /api/meetings` and `/api/meetings/search` take `archived=true`, beside `starred=true`.
- `POST /api/meetings/archive` takes `{"before": <unix ms>}` and archives, in one `UPDATE`, every meeting that is not archived and started before it.
  The `UPDATE` sets `updated_at` too, because `cloud_fingerprint` (`meetings.rs:430`) watches it to tell the Cloud panel that meetings wait to push.
  It skips the markdown export, since the file does not show archive state.
  With `"preview": true` it returns `{"count": n}` and changes nothing, so the confirmation can name the number.
  MTG-23 added `"label_id"`, which limits it to meetings with that label, and either field may be left out but not both.
  The Library fans out one request per meeting for bulk delete and bulk label (`web/src/lib/api/meetings.ts:228-260`), which works for a page of at most 100 but not for 1,300 meetings.

Neither route archives the meeting being recorded.
`PATCH` answers 409 for the id `state.meetings.active_recording()` returns, and the date rule leaves that id out, since a recording that began before the chosen midnight would otherwise vanish mid-meeting.

### The Library

The sidebar gains an Archive link after Starred, routed to `/archive` as `<Library archivedOnly />`, the way `/starred` is `<Library starredOnly />` (`web/src/router.tsx:48`).
Its icon is lucide's `Archive`, beside the `Star` the sidebar already imports.

The Archive view's header reads Archive, and its search box searches only archived meetings.
Empty, it reads "No archived meetings", with a line on how to archive one.

Each card's kebab menu (`web/src/components/library/MeetingCardActions.tsx:117`) gains Archive above Delete, or Unarchive in the Archive view.
The card leaves the list at once.
The web UI has no toast, so a mistake is undone from the Archive view.
The card of the meeting being recorded has no Archive item.

The selection bar (`web/src/components/library/SelectionActionBar.tsx`) gains Archive between the label buttons and Delete, or Unarchive in the Archive view.
It fans out `PATCH` per meeting, as bulk label does, and asks for no confirmation, since nothing is lost.

A button in the Archive view's header, left of the search box, "Archive older meetings", opens a panel under it with a date field and the confirmation "Archive 1,212 meetings that started before Jan 1, 2026?"
The count comes from the preview call.
The bound is the chosen date's midnight in the local time zone, and the field cannot pick a date after today.

A label's sidebar menu has Archive meetings above Delete (MTG-23), so a Granola folder imported as a label can be archived whole.
It confirms in the menu like Delete, "Archive?" over "Archives 312 meetings", with the count from the preview call.

MTG-24 tidies labels after that.
A sidebar label counts only its unarchived meetings, and the delete confirmation still counts all of them.
`GET /api/labels` returns `archived_count` beside `meeting_count` for this.
A label with archived meetings and no others moves to a collapsed Archived labels group under the list, derived from the meetings with nothing stored.
It returns to the list once one of its meetings is unarchived or a new meeting gets it, which is also how a recreated label of the same name combines with it, since names are unique.
In the Archive view a label opens `/archive/label/:id` and counts its archived meetings, and an archived label always opens there.
There the Archive nav item stays highlighted beside the label row, and clicking the selected label again goes back to the whole Archive, as it goes back to the whole Library outside it (MTG-26).

The meeting page header (`web/src/routes/MeetingPost.tsx:496`) gains an Archive icon button before Delete.
On an archived meeting it becomes `ArchiveRestore` on the selected-blue background, and its tooltip reads "Archived Oct 27, 2026", so a meeting opened from a link shows why the Library does not list it.

## The webapp (SYNC-21 and SYNC-22)

The webapp is read-only, and sync is one-way (`docs/.planning/sync-cloud-mirror-design.md:19`).
A meeting's archive state reaches the phone in its pushed body as `meeting.archived_at`, as `starred` and `labels` already do.
The webapp never archives.

### Reading archive state (SYNC-21)

Each of the three reads checks the latest version's body:

```sql
(v.body -> 'meeting' ->> 'archived_at' is not null) = only_archived
```

- `yogurt_library` (`crates/yogurt-cloud/schema.sql:518`) puts it in the `where l.live` of its `ordered` step, so `total` and the pager count only what the view shows.
- `yogurt_search` already reads each hit's latest version to drop a meeting deleted since the index caught up (`schema.sql:624`), and the check joins it.
- `yogurt_retrieve` does the same in its match path (`:666`) and its fallback (`:676-688`), with archived meetings always left out.

None of these functions is in `yogurt_search_signature` (`schema.sql:288`), and the index gains no column, so the deploy rebuilds no index rows.
Because the check reads the latest version, a search hit from an index row that has not caught up still hides an archived meeting.
`yogurt_library` and `yogurt_search` gain `only_archived boolean default false`, named like `only_device`.
A new parameter changes the function's signature, so the schema drops the old one first, which the upgrade test covers.

The webapp gains a row of filter pills under the device pills (`cloud/src/ui.tsx:272`), which today has none.
SYNC-21 puts two pills in it, All and Archive.
Archive worked under All devices too until SYNC-23 moved it to the per-Mac row.
Its search searches only the archive.
On a phone the row scrolls sideways, as the device pills do, and on a desktop it wraps in the 300px aside.
With a Mac picked it neither scrolls nor wraps, since either would clip the Labels list, and its pills fit a 320px phone.
The filter sits in component state beside the picked device, and the as-of view shows each meeting's archive state at that push, since the state lives in the version.

### Each Mac's Starred and Labels (SYNC-22)

Label ids are local to each Mac, and two Macs can each have a "Clients" label that means different things.
So the label pills appear only once a Mac is picked.
SYNC-23 later offered Starred under All devices as well, listing every Mac's starred meetings.

| Picked | Filter pills |
| --- | --- |
| All devices | All · Starred |
| One Mac | All · Starred · Archive, and a Labels button at the end of the row |

A new RPC, `yogurt_labels(token, only_device)`, returns that Mac's labels from its latest live, unarchived versions, with `id`, `name`, and `color`, sorted by name.
`yogurt_library` and `yogurt_search` gain `only_starred boolean default false` and `only_label text default null`.
Since SYNC-23, `only_starred` applies without `only_device` and `only_label` still needs one, and the `only_` names keep clear of `yogurt_library`'s `starred` output column, which Postgres would reject as a repeated name.
The Labels button opens a list of that Mac's labels with their colors, because a Mac can have too many labels for a row of pills.
Picking one closes the list, and the button then shows that label's color and name in the selected style.
SYNC-27 made a label narrow the picked pill, so Archive with a label lists that label's archived meetings.
With Archive picked, `yogurt_labels(..., only_archived => true)` lists the labels of the Mac's archived meetings, and moving into or out of Archive takes the label off, since the two lists differ.
All labels at the top of the list takes the label off.
Changing the Mac takes the label off, because the old Mac's label means nothing on the new one.
Starred stays, and so does Archive unless the new pick is All devices.
With a Mac picked, rows drop their device tag, since every row would show the same one.

## Cost

Archiving changes a meeting's pushed body, so the next push sends a new version of each archived meeting.
Today's Cloud holds 85 versions of 50 meetings, averaging 5.4 KB each compressed, in a 13 MB database.
Archiving 1,300 imported meetings after they were pushed adds about 1,300 versions, roughly 7 MB at today's average and more for imports with long transcripts.
Each new version also queues its meeting's search row for a rebuild, which catch-up works through a few seconds at a time over the following searches.
Archiving before the first push of an import sends each meeting once, already archived.
SYNC-14's 30-day pruning would drop the older versions.

## Testing

MTG-21 adds `meetings.rs` tests for the filter in list, search, the day match, and `retrieve`, and for the date rule's preview, bound, `updated_at`, and the recording it skips.
An API test checks that the Granola import skips an archived meeting, and another that `PATCH` refuses the recording.
Playwright covers archiving from a card and by multi-select, the Archive view and its search, Unarchive, the greeting count, and the date panel's count.
The PR shows light and dark screenshots of the sidebar, the Archive view, the date panel, and the archived meeting header.

SYNC-21 adds schema tests that `yogurt_library`, `yogurt_search`, and `yogurt_retrieve` hide an archived meeting, that `only_archived => true` shows only archived ones with a matching `total`, and that unarchiving brings it back.
The upgrade test covers the changed signatures, and `cloud/e2e` specs cover the Archive pill on a phone and a desktop.

SYNC-22 adds schema tests for `yogurt_labels`, for the filters ignored without a device, and for a label id from another Mac matching nothing.
Its e2e specs cover the filter pills and the reset on a Mac change.

## Docs

- `docs/FEATURES.md`: the Library row gains `/archive` and `archived=true`, Chat across meetings says it skips archived meetings, and the Cloud rows gain the filters.
- `docs/CLOUD-SYNC.md`: the pushed body carries `archived_at`, and the webapp's filters.
- `docs/AI-INTEGRATION.md`: the new routes.

## Decided after review

- The Archive view has no label filter inside it, matching Starred, whose views never combine with a label.
  Search is how to find an old meeting in the archive.
- An archived starred meeting leaves Starred and keeps its star, so Unarchive puts it back.
- Archive older meetings has no Undo.
  The confirmation shows the count first, and any meeting can be unarchived.
- The webapp's label list has no search box.
- The filter pills keep the device pills' style, so the two rows stay consistent.
