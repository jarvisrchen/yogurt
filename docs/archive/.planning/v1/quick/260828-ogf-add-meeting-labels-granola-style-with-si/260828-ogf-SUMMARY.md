---
phase: quick
plan: 260828-ogf
subsystem: ui
tags: [sqlite, axum, react, tanstack-query, labels, library]
requires: []
provides:
  - "labels + meeting_labels SQLite tables with cascade FKs (V007 migration)"
  - "LabelRepo (find-or-create, rename/recolor, delete, set_for_meeting)"
  - "GET/POST /api/labels, PATCH/DELETE /api/labels/:id REST surface"
  - "label_ids on PATCH /api/meetings/:id, hydrated Meeting.labels field"
  - "LabelPicker popover reused across card, live header, post header"
  - "Sidebar Labels section with rename/recolor/delete and /label/:id filter"
affects: [yogurt-db, yogurt-server, web]
key-files:
  created:
    - crates/yogurt-db/migrations/V007__labels.sql
    - crates/yogurt-db/src/labels.rs
    - crates/yogurt-server/src/api/labels.rs
    - crates/yogurt-server/tests/labels_api.rs
    - web/src/lib/api/labels.ts
    - web/src/components/labels/LabelChip.tsx
    - web/src/components/labels/LabelPicker.tsx
    - web/src/components/labels/LabelPicker.test.tsx
    - web/src/components/labels/MeetingLabels.tsx
    - web/src/components/library/SidebarLabelRow.tsx
    - web/src/components/library/Sidebar.labels.test.tsx
  modified:
    - crates/yogurt-db/src/meetings.rs
    - crates/yogurt-db/src/migrations.rs
    - crates/yogurt-db/src/lib.rs
    - crates/yogurt-server/src/api/meetings.rs
    - crates/yogurt-server/src/api/mod.rs
    - crates/yogurt-server/src/routes.rs
    - crates/yogurt-server/src/state.rs
    - web/src/lib/api/meetings.ts
    - web/src/components/library/MeetingCard.tsx
    - web/src/components/library/MeetingCardActions.tsx
    - web/src/components/library/Sidebar.tsx
    - web/src/routes/Library.tsx
    - web/src/routes/Meeting.tsx
    - web/src/routes/MeetingPost.tsx
    - web/src/router.tsx
key-decisions:
  - "find_or_create returns (Label, bool created) so the REST handler can pick 201 vs 200 without a second query"
  - "ApiError moved from api/meetings.rs to api/mod.rs (pub(crate)) and shared with api/labels.rs; the generic anyhow-string mapping treats \"label not found\" as 400 (bad label_ids reference on an existing meeting), while the labels router's own PATCH handler special-cases its own \"unknown id in the URL\" outcome to 404 before falling through to the generic mapping"
  - "web/src/lib/api/labels.ts imports meetingsKey/json/Label/LabelColor from meetings.ts (one-directional); meetings.ts invalidates the labels query by the literal key [\"labels\"] instead of importing labelsKey back, avoiding a meetings.ts <-> labels.ts circular import"
  - "LABEL_COLORS: blue/matcha/straw reuse existing --color-*soft design tokens; lilac/honey/slate are inline hex (no tokens exist yet for those three)"
  - "SidebarLabelRow extracted into its own file up front (plan's ~250-line Sidebar threshold) since rename+recolor+delete state made the row non-trivial"
metrics:
  duration: "~55 min"
  tasks: 3
  files: 25
completed: 2026-08-28
---

# Quick Task 260828-ogf: Add Meeting Labels (Granola-style) Summary

Workspace-level named meeting labels (SQLite `labels` + `meeting_labels` tables, `/api/labels*` REST surface, and a reusable `LabelPicker` mounted on the Library card, live meeting header, and post-meeting header) with a Sidebar section supporting rename/recolor/delete and a `/label/:id` filter route.

## What Changed

### Task 1: DB layer - labels tables, LabelRepo, labels on Meeting

- Commit `09008c8`
- `V007__labels.sql`: `labels` (unique case-insensitive name index) + `meeting_labels` (cascade FKs both directions)
- `LabelRepo`: `list_with_counts` (LEFT JOIN + GROUP BY), `find_or_create` (case-insensitive match, palette-color rotation), `update` (rename/recolor with duplicate-name guard), `delete`, `set_for_meeting` — the last backed by a shared `set_for_meeting_conn(conn, ...)` free function so `MeetingRepo::patch` and `LabelRepo::set_for_meeting` share one transactional implementation
- `Meeting` gains `labels: Vec<Label>` hydrated via one extra `labels_for()` query in `get`/`list`/`search`; `MeetingPatch` gains `label_ids: Option<Vec<String>>` — a label-only patch (no other fields) still bumps `updated_at` because the early-return-on-empty-`sets` guard was narrowed to `sets.is_empty() && label_ids.is_none()`
- 7 new unit tests in `labels.rs` covering case-insensitivity, sorting, counts, rename conflicts, cascade-delete in both directions, and the label-only-patch path

### Task 2: Server REST - /api/labels and label_ids on meeting PATCH

- Commit `2f283d2`
- `ApiError` moved from `api/meetings.rs` to `api/mod.rs` (`pub(crate)`) and shared by both `api::meetings` and the new `api::labels`
- `api/labels.rs`: `GET/POST /api/labels` (find-or-create returns 201 when newly created, 200 when an existing case-insensitive match was returned), `PATCH/DELETE /api/labels/:id`
- `AppState` gains `label_repo: Arc<LabelRepo>` alongside `meeting_repo` in all three constructors (`production`, `production_warmed` inherits it, `in_memory`), plus the four hand-built `AppState` literals in test helpers/integration tests that don't go through those constructors
- New `tests/labels_api.rs`: find-or-create idempotency, full apply/rename/delete-cascade round trip through a real meeting, unknown-label-id-on-meeting-patch → 400, unauthenticated → non-2xx

### Task 3: Web - API hooks, LabelPicker, chips, sidebar, /label/:id, three mount points

- Commit `4666f85`
- `lib/api/labels.ts` + `Label`/`LabelColor` types added to `lib/api/meetings.ts` (exported `json<T>()` helper reused); `useSetMeetingLabels` mutation invalidates both the meetings and labels caches
- `LabelChip`, `LabelPicker` (search/create/toggle popover, click-outside + Escape, `stopPropagation` so it survives inside a card `<Link>`), `MeetingLabels` (chips + "+ Label" trigger) — the latter two mounted on `MeetingCard`/`MeetingCardActions` (new Tag hover-action), the live meeting header, and the post-meeting header
- `Sidebar` Labels section + `SidebarLabelRow` (rename inline input, 6-swatch recolor row, delete with the same 3s-auto-revert "Delete?"/"Cancel" pattern as `MeetingCardActions`)
- `/label/:labelId` route filters `Library`; bounces to `/` via `<Navigate replace>` if the label was deleted out from under the URL
- `LabelPicker.test.tsx` (3 cases) + `Sidebar.labels.test.tsx` (2 cases); existing `Meeting.test.tsx`/`MeetingPost.test.tsx`/`MeetingPost.autosave.test.tsx` stub the new `useSetMeetingLabels` export now that `MeetingLabels` mounts in both headers; 3 existing Meeting fixture files updated with `labels: []`

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Two existing `MeetingPatch` call sites needed the new field**
- **Found during:** Task 1, immediately after adding `label_ids` to `MeetingPatch`
- **Issue:** `crates/yogurt-server/src/enhance.rs` and `crates/yogurt-server/src/api/meetings.rs` construct `MeetingPatch` with every field spelled out (no `..Default::default()`), so `cargo build --workspace` failed with "missing field `label_ids`"
- **Fix:** added `label_ids: None` at both call sites (Task 2 later changed the `api/meetings.rs` one to forward `body.label_ids` for real)
- **Files modified:** crates/yogurt-server/src/enhance.rs, crates/yogurt-server/src/api/meetings.rs
- **Commit:** 09008c8 (Task 1)

**2. [Rule 3 - Blocking] Four hand-built `AppState` literals needed `label_repo`**
- **Found during:** Task 2, after adding `label_repo` to `AppState`
- **Issue:** `test_support.rs`, `tests/e2e_synthetic_audio.rs`, `tests/meeting_ws.rs`, `tests/meeting_ws_auth.rs` construct `AppState` directly (not via the `production`/`in_memory` constructors) and don't set the new field
- **Fix:** added `let label_repo = Arc::new(yogurt_db::LabelRepo::new(db.clone()));` + `label_repo,` in the struct literal at each site
- **Files modified:** crates/yogurt-server/src/test_support.rs, crates/yogurt-server/tests/e2e_synthetic_audio.rs, crates/yogurt-server/tests/meeting_ws.rs, crates/yogurt-server/tests/meeting_ws_auth.rs
- **Commit:** 2f283d2 (Task 2)

**3. [Rule 1 - Bug] LabelPicker's trigger button toggle fought its own outside-click listener**
- **Found during:** Task 3, while wiring the Tag button in `MeetingCardActions` and the "+ Label" button in `MeetingLabels`
- **Issue:** `LabelPicker`'s outside-click detection listens on `document` `mousedown` and only excludes its own popover wrapper — not the external button that opens/closes it. Clicking the trigger button to close an open picker would fire the mousedown-based `onClose` first (button is "outside" the popover), then the button's own `onClick` handler would toggle state back open in the same gesture
- **Fix:** added `onMouseDown={(e) => e.stopPropagation()}` (and `preventDefault`) to both trigger buttons so the native mousedown never reaches the document-level listener; the click handler's toggle then runs against the correct prior state
- **Files modified:** web/src/components/library/MeetingCardActions.tsx, web/src/components/labels/MeetingLabels.tsx
- **Commit:** 4666f85 (Task 3)

**4. [Rule 3 - Blocking] Circular import between meetings.ts and labels.ts**
- **Found during:** Task 3, while wiring `useSetMeetingLabels`'s cache invalidation
- **Issue:** the plan's shape has `labels.ts` importing `meetingsKey`/types from `meetings.ts`, and `meetings.ts` needing `labelsKey` from `labels.ts` to invalidate the labels list after a `label_ids` patch — a two-way ES module cycle
- **Fix:** kept the import one-directional (`labels.ts` → `meetings.ts`); `meetings.ts`'s `useSetMeetingLabels` invalidates the literal `["labels"]` key instead of importing the `labelsKey` constant — TanStack Query matches query keys structurally, so this is equivalent to importing the const
- **Files modified:** web/src/lib/api/meetings.ts
- **Commit:** 4666f85 (Task 3)

**5. [Rule 3 - Blocking] Existing Meeting/MeetingPost test suites broke when MeetingLabels mounted**
- **Found during:** Task 3, first full `pnpm test` run
- **Issue:** `Meeting.test.tsx`, `MeetingPost.test.tsx`, and `MeetingPost.autosave.test.tsx` each fully replace `../lib/api/meetings` via `vi.mock(...)`; once `<MeetingLabels>` started mounting in both headers and calling `useSetMeetingLabels()`, those mocks lacked the export and every test in all three files failed
- **Fix:** added `useSetMeetingLabels: () => ({ mutate: vi.fn() })` to each file's existing mock factory
- **Files modified:** web/src/routes/Meeting.test.tsx, web/src/routes/MeetingPost.test.tsx, web/src/routes/MeetingPost.autosave.test.tsx
- **Commit:** 4666f85 (Task 3)

**6. [Rule 1 - Bug] act() warning on the LabelPicker create-and-apply test**
- **Found during:** Task 3, first `pnpm test` run
- **Issue:** manually chaining two `await Promise.resolve()` after firing the Enter key let the state update from the async `createAndApply()` land outside React's `act()` wrapper, producing a console warning
- **Fix:** replaced the manual promise chain with `await waitFor(() => expect(setMutate).toHaveBeenCalledWith(...))`, which flushes pending state updates correctly
- **Files modified:** web/src/components/labels/LabelPicker.test.tsx
- **Commit:** 4666f85 (Task 3)

---

**Total deviations:** 6 auto-fixed (4 Rule 3 blocking-build/test fixes, 2 Rule 1 bug fixes)
**Impact on plan:** All auto-fixes were required collateral of the plan's own struct/field additions or a genuine interaction bug found while building the exact UI the plan specified. No scope creep beyond the plan's file list.

## Issues Encountered

None beyond the deviations above.

## Verification

- `cargo test --workspace`: 276 passed, 3 ignored (0 failed)
- `cargo clippy --workspace --all-targets -- -D warnings`: no issues found
- `cargo fmt --check`: clean (ran `cargo fmt` once mid-Task-2 to fix formatting drift)
- `pnpm -C web typecheck`: clean
- `pnpm -C web test`: 206 passed (34 test files), 0 failed

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Labels are fully wired end-to-end (DB → REST → UI) and covered by both Rust integration tests and web unit tests
- No known stubs or deferred UI states
- Minor known limitation (not a correctness issue): the Sidebar's Labels list has no independent scroll region — a very long label list will grow the whole sidebar past the viewport rather than scrolling internally. Left as-is per the plan's scope; worth a follow-up if the label count grows large in practice.

---
*Phase: quick*
*Completed: 2026-08-28*

## Self-Check: PASSED

- All 11 newly-created files present on disk (DB migration/repo, server router/test, web api/components/tests).
- Commits 09008c8, 2f283d2, 4666f85 all present on `gsd/autonomous`.

## Orchestrator E2E pass (Chrome, real backend)

Verified against the running dev server: create label from a card, chip on card, sidebar count, `/label/:id` filter, rename, recolor, delete, and the picker in both the live and post-meeting headers.
Three defects found and fixed in commit c4b0a4e:

- Sidebar labels container had `overflow-y-auto`, which swallowed the absolutely-positioned row menu (rename/recolor/delete were invisible).
- `LabelPicker` default anchor lacked `top-full left-0`, so in the meeting headers it opened over the title.
- Auto-color picked by label count, so a recolored label and the next new label collided; now picks the first unused palette entry.

Test labels created during the pass were deleted from the local DB afterwards.
