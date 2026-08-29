---
phase: quick-260828-ogf
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/yogurt-db/migrations/V007__labels.sql
  - crates/yogurt-db/src/migrations.rs
  - crates/yogurt-db/src/labels.rs
  - crates/yogurt-db/src/meetings.rs
  - crates/yogurt-db/src/lib.rs
  - crates/yogurt-server/src/api/labels.rs
  - crates/yogurt-server/src/api/meetings.rs
  - crates/yogurt-server/src/api/mod.rs
  - crates/yogurt-server/src/routes.rs
  - crates/yogurt-server/tests/labels_api.rs
  - web/src/lib/api/meetings.ts
  - web/src/lib/api/labels.ts
  - web/src/components/labels/LabelChip.tsx
  - web/src/components/labels/LabelPicker.tsx
  - web/src/components/labels/LabelPicker.test.tsx
  - web/src/components/labels/MeetingLabels.tsx
  - web/src/components/library/Sidebar.tsx
  - web/src/components/library/MeetingCard.tsx
  - web/src/components/library/MeetingCardActions.tsx
  - web/src/routes/Library.tsx
  - web/src/routes/Meeting.tsx
  - web/src/routes/MeetingPost.tsx
  - web/src/router.tsx
autonomous: true
requirements: []

must_haves:
  truths:
    - "A user can create a label by typing a new name in the picker, and that label immediately appears in the left sidebar under a Labels section with a color dot and meeting count"
    - "A user can add/remove labels on any meeting from the Library card hover actions, from the live meeting header, and from the post-meeting header, using the same picker"
    - "Clicking a label in the sidebar filters the Library to meetings carrying that label (route /label/:labelId), with the sidebar row highlighted as active"
    - "Label chips render on Library meeting cards"
    - "Renaming or deleting a label from the sidebar updates every meeting that carried it; deleting a meeting removes its label associations"
    - "cargo test --workspace, pnpm typecheck, and pnpm test all pass"
  artifacts:
    - path: "crates/yogurt-db/migrations/V007__labels.sql"
      provides: "labels + meeting_labels tables"
      contains: "meeting_labels"
    - path: "crates/yogurt-db/src/labels.rs"
      provides: "LabelRepo: list_with_counts, find_or_create, rename/recolor, delete, set_for_meeting, plus unit tests"
      contains: "set_for_meeting"
    - path: "crates/yogurt-server/src/api/labels.rs"
      provides: "GET/POST /api/labels, PATCH/DELETE /api/labels/{id}"
    - path: "crates/yogurt-server/tests/labels_api.rs"
      provides: "REST contract tests for labels + label_ids on meeting PATCH"
    - path: "web/src/components/labels/LabelPicker.tsx"
      provides: "reusable popover: search/create + toggle labels for one meeting"
    - path: "web/src/components/library/Sidebar.tsx"
      provides: "Labels nav section"
  key_links:
    - from: "web/src/components/labels/LabelPicker.tsx"
      to: "PATCH /api/meetings/:id { label_ids }"
      via: "useSetMeetingLabels"
      pattern: "label_ids"
    - from: "crates/yogurt-db/src/meetings.rs (row_to_meeting/list/get)"
      to: "meeting_labels"
      via: "labels_for_meetings hydration"
      pattern: "labels_for"
---

<objective>
Add Granola-style meeting labels to yogurt.
Labels are workspace-level named tags with a color.
A meeting can carry any number of labels.
Labels are managed from the left sidebar and applied from three surfaces: the Library meeting card, the live meeting header, and the post-meeting header.

Purpose: let users organize meetings (e.g. "1:1", "Customer", "Standup") without folders, matching a feature users know from Granola.
Output: DB tables + Rust repo + REST endpoints + React sidebar section, picker, and chips, all tested.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@CLAUDE.md
@crates/yogurt-db/src/meetings.rs
@crates/yogurt-db/src/migrations.rs
@crates/yogurt-server/src/api/meetings.rs
@crates/yogurt-server/src/routes.rs
@crates/yogurt-server/tests/meetings_api.rs
@web/src/lib/api/meetings.ts
@web/src/components/library/Sidebar.tsx
@web/src/components/library/MeetingCard.tsx
@web/src/components/library/MeetingCardActions.tsx
@web/src/routes/Library.tsx
@web/src/routes/Meeting.tsx
@web/src/routes/MeetingPost.tsx
@web/src/router.tsx

House rules that apply to every task below:
- Never use the em dash character; use a plain dash.
- Match the existing code style: doc comments at file top explaining the why, Tailwind design tokens (`text-ink`, `bg-paper`, `border-line`, `bg-blsoft`, `text-blue`, `text-mut`, `rounded-button`, `rounded-pill`, `rounded-card`, `shadow-pop`).
- No new dependencies. No new crates. No subprocesses.
- Keep it small. No abstractions with one implementation. Reuse the existing `json<T>()` helper in `web/src/lib/api/meetings.ts` (export it if needed) and the existing `ApiError` pattern in `api/meetings.rs` (move it to `api/mod.rs` or duplicate a minimal copy; either is fine, prefer moving and re-exporting).
- Commit each task atomically with a conventional commit message (`feat(db): ...`, `feat(server): ...`, `feat(web): ...`). Do not add a Co-Authored-By trailer.
</context>

<tasks>

<task type="auto">
  <name>Task 1: DB layer - labels tables, LabelRepo, labels on Meeting</name>
  <files>
    crates/yogurt-db/migrations/V007__labels.sql
    crates/yogurt-db/src/migrations.rs
    crates/yogurt-db/src/labels.rs
    crates/yogurt-db/src/meetings.rs
    crates/yogurt-db/src/lib.rs
  </files>
  <action>
1. Migration `V007__labels.sql`, registered in `migrations.rs` after V006:
```sql
CREATE TABLE IF NOT EXISTS labels (
    id         TEXT PRIMARY KEY,
    name       TEXT NOT NULL,
    color      TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_labels_name_nocase ON labels(name COLLATE NOCASE);
CREATE TABLE IF NOT EXISTS meeting_labels (
    meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
    label_id   TEXT NOT NULL REFERENCES labels(id)   ON DELETE CASCADE,
    PRIMARY KEY (meeting_id, label_id)
);
CREATE INDEX IF NOT EXISTS idx_meeting_labels_label ON meeting_labels(label_id);
```
(foreign_keys pragma is already ON in `Db::open` / `open_in_memory`.)

2. New `crates/yogurt-db/src/labels.rs`:
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label { pub id: String, pub name: String, pub color: String }

#[derive(Debug, Clone, Serialize)]
pub struct LabelWithCount { #[serde(flatten)] pub label: Label, pub meeting_count: i64 }

#[derive(Clone)] pub struct LabelRepo { db: Db }
impl LabelRepo {
    pub fn new(db: Db) -> Self;
    /// All labels, name ASC (COLLATE NOCASE), with meeting_count via LEFT JOIN meeting_labels GROUP BY.
    pub fn list_with_counts(&self) -> Result<Vec<LabelWithCount>>;
    /// Trim name; bail on empty or > 40 chars. If a label with the same name (NOCASE) exists, return it unchanged. Otherwise insert with ULID id, `color` (if None pick from COLORS by count-of-existing-labels % COLORS.len()), created_at = now ms.
    pub fn find_or_create(&self, name: &str, color: Option<&str>) -> Result<Label>;
    /// Update name and/or color. Same validation as create. Duplicate name (NOCASE, different id) -> bail "label name already exists". Unknown id -> bail "label not found".
    pub fn update(&self, id: &str, name: Option<&str>, color: Option<&str>) -> Result<Label>;
    /// Returns Ok(true) if a row was removed. meeting_labels rows cascade.
    pub fn delete(&self, id: &str) -> Result<bool>;
    /// Replace the meeting's label set inside one transaction (DELETE then INSERT OR IGNORE). Unknown label ids -> bail "label not found". Does not touch meetings.updated_at (the caller MeetingRepo::patch does).
    pub fn set_for_meeting(&self, meeting_id: &str, label_ids: &[String]) -> Result<()>;
}
/// Palette keys understood by the web LabelChip; store the key, not a hex.
pub const COLORS: [&str; 6] = ["blue", "matcha", "straw", "lilac", "honey", "slate"];
```
`color` must be validated to be one of `COLORS` (bail "invalid color" otherwise) in both create and update.
Note: `Db::with_conn` gives `&Connection`, so use `conn.execute_batch("BEGIN")`/`COMMIT` or `unchecked_transaction()` for the set_for_meeting transaction.

3. `meetings.rs`:
   - Add `pub labels: Vec<Label>` to `Meeting` (serde default on deserialize).
   - Add `#[serde(default, skip_serializing_if = "Option::is_none")] pub label_ids: Option<Vec<String>>` to `MeetingPatch`.
   - Add a private `fn labels_for(conn: &Connection, meeting_ids: &[String]) -> rusqlite::Result<HashMap<String, Vec<Label>>>` that runs one query: `SELECT ml.meeting_id, l.id, l.name, l.color FROM meeting_labels ml JOIN labels l ON l.id = ml.label_id WHERE ml.meeting_id IN (...) ORDER BY l.name COLLATE NOCASE` (build the placeholder list; for `get` there is one id, for `list`/`search` pass all ids; if empty, return empty map without querying).
   - `row_to_meeting` sets `labels: Vec::new()`; `get`, `list`, and `search` then hydrate via `labels_for` before returning. Do this inside the same `with_conn` closure so it is one lock.
   - In `patch`, when `patch.label_ids` is `Some(ids)`: after the dynamic UPDATE (and even when `sets` is empty - restructure so a label-only patch still bumps `updated_at` and does not early-return), call the same logic as `LabelRepo::set_for_meeting` on `conn` (factor a free fn `labels::set_for_meeting_conn(conn, meeting_id, ids)` that both `LabelRepo::set_for_meeting` and `MeetingRepo::patch` call). Verify the meeting exists first (the UPDATE returning 0 rows already bails "meeting not found" when `sets` is non-empty; for a label-only patch do an explicit `SELECT 1 FROM meetings WHERE id = ?`).
   - Keep the FTS sync block as-is.

4. `lib.rs`: `pub mod labels;` and `pub use labels::{Label, LabelRepo, LabelWithCount};`.

5. Tests in `labels.rs` (`#[cfg(test)]`, use `Db::open_in_memory()` and `MeetingRepo` to create meetings):
   - find_or_create returns same id for "Sales" and "sales"; rejects empty; assigns a palette color.
   - set_for_meeting + MeetingRepo::get returns labels sorted by name; list_with_counts shows count 1; replacing with [] clears.
   - update renames; renaming to another existing name bails.
   - delete label removes it from meeting; deleting meeting drops meeting_labels rows (count goes to 0).
   - MeetingRepo::patch with label_ids only (no other fields) applies labels and bumps updated_at.
   Also fix any existing test fixtures in the workspace that construct `Meeting { .. }` literally (grep `stt_engine:` in `crates/`) by adding `labels: vec![]`.
  </action>
  <verify>cargo test -p yogurt-db  &&  cargo build --workspace</verify>
  <done>Migration applies on fresh in-memory DB; all yogurt-db tests pass; workspace compiles.</done>
</task>

<task type="auto">
  <name>Task 2: Server REST - /api/labels and label_ids on meeting PATCH</name>
  <files>
    crates/yogurt-server/src/api/labels.rs
    crates/yogurt-server/src/api/meetings.rs
    crates/yogurt-server/src/api/mod.rs
    crates/yogurt-server/src/routes.rs
    crates/yogurt-server/src/state.rs
    crates/yogurt-server/tests/labels_api.rs
  </files>
  <action>
1. `state.rs`: add `pub label_repo: Arc<LabelRepo>` to `AppState`, constructed next to `meeting_repo` in both constructors (`LabelRepo::new(db.clone())`).

2. `api/meetings.rs`: add `label_ids: Option<Vec<String>>` to `PatchBody` and forward into `MeetingPatch { label_ids: body.label_ids, .. }`. Map repo errors containing "label not found" or "invalid color" or "already exists" to 400 in `ApiError::from` (extend the existing string match; "label not found" must map to BadRequest, not 404, because the meeting exists). Make `ApiError` `pub(crate)` and move it to `api/mod.rs` so `labels.rs` can reuse it (update the `use` in meetings.rs).

3. New `api/labels.rs` with `pub fn router() -> Router<AppState>`:
   - `GET /api/labels` -> `Json<Vec<LabelWithCount>>` (200).
   - `POST /api/labels` body `{ name: String, color?: String }` -> `Json<Label>`; 201 when newly created, 200 when an existing label was returned (find_or_create semantics; detect by comparing count before/after or by returning a bool from the repo - simplest: add `pub fn find_or_create(&self, ..) -> Result<(Label, bool /*created*/)>`; adjust Task 1 accordingly). Empty name -> 400.
   - `PATCH /api/labels/{id}` body `{ name?: String, color?: String }` -> `Json<Label>`; unknown id -> 404; duplicate name / invalid color -> 400.
   - `DELETE /api/labels/{id}` -> 204, or 404 when nothing was deleted.
   All handlers use `spawn_blocking` like the meetings handlers.
   Mount it in `routes.rs` right after `.merge(crate::api::meetings::router())` (inside the same session-token-protected group).

4. Integration test `tests/labels_api.rs` (copy the `spawn_server` helper from `tests/meetings_api.rs`; keep it minimal):
   - POST /api/labels {name:"Sales"} -> 201; POST again {name:"sales"} -> 200 same id; GET /api/labels lists one with meeting_count 0.
   - POST /api/meetings, PATCH /api/meetings/{id} {label_ids:[lid]} -> 200 and body.labels[0].name == "Sales"; GET /api/labels meeting_count == 1; GET /api/meetings list row carries labels.
   - PATCH /api/labels/{lid} {name:"Customers", color:"straw"} -> 200; GET /api/meetings/{id} shows renamed label.
   - PATCH meeting with unknown label id -> 400.
   - DELETE /api/labels/{lid} -> 204; meeting now has empty labels; DELETE again -> 404.
   - All requests without the bearer token -> 401/403 (whatever the existing middleware returns; assert non-2xx). One assertion is enough.
  </action>
  <verify>cargo test -p yogurt-server --test labels_api --test meetings_api  &&  cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -5</verify>
  <done>All label endpoints behave per the contract; existing meetings_api tests still pass; clippy clean.</done>
</task>

<task type="auto">
  <name>Task 3: Web - API hooks, LabelPicker, chips, sidebar section, /label/:id filter, three mount points</name>
  <files>
    web/src/lib/api/meetings.ts
    web/src/lib/api/labels.ts
    web/src/components/labels/LabelChip.tsx
    web/src/components/labels/LabelPicker.tsx
    web/src/components/labels/LabelPicker.test.tsx
    web/src/components/labels/MeetingLabels.tsx
    web/src/components/library/Sidebar.tsx
    web/src/components/library/MeetingCard.tsx
    web/src/components/library/MeetingCardActions.tsx
    web/src/routes/Library.tsx
    web/src/routes/Meeting.tsx
    web/src/routes/MeetingPost.tsx
    web/src/router.tsx
  </files>
  <action>
1. `lib/api/meetings.ts`: export the `json<T>()` helper; add `export interface Label { id: string; name: string; color: LabelColor }` where `export type LabelColor = "blue" | "matcha" | "straw" | "lilac" | "honey" | "slate"`; add `labels: Label[]` to `Meeting`; add `label_ids?: string[]` to `MeetingPatch`. Add hook `useSetMeetingLabels()` = mutation `({id, label_ids}) => meetingsApi.patch(id, {label_ids})`, on success invalidate `meetingsKey` (this also covers search keys since they share the `["meetings", ...]` prefix) and `labelsKey`, and `setQueryData(meetingKey(m.id), m)`. Update every test fixture in `web/src` that builds a `Meeting` literal (grep `stt_engine:`) to include `labels: []`.

2. `lib/api/labels.ts`: `export const labelsKey = ["labels"] as const;` `LabelWithCount = Label & { meeting_count: number }`; `labelsApi = { list, create(name, color?), update(id, {name?, color?}), delete(id) }` over `/api/labels*` using `json<T>()`; hooks `useLabels()` (staleTime 5s), `useCreateLabel()`, `useUpdateLabel()`, `useDeleteLabel()` - each mutation invalidates `labelsKey` and `meetingsKey` on success (rename/delete changes every meeting's embedded labels).

3. `components/labels/LabelChip.tsx`: `LabelChip({ label, size?: "sm" | "md", onRemove? })`. Export `LABEL_COLORS: Record<LabelColor, { bg: string; fg: string }>` using the existing tokens where they exist (`blue` -> bg `var(--color-blsoft)` fg `var(--color-blue)`; `matcha` -> mtsoft/matcha; `straw` -> strsoft/straw) and inline hex for the three new ones (`lilac` bg `#F3E8FB` fg `#8A5BB8`; `honey` bg `#FBF0D6` fg `#A67C1B`; `slate` bg `#E8ECF0` fg `#4E5D6B`). Unknown key falls back to `line`/`mut`. Render as `<span class="inline-flex items-center gap-1 rounded-pill px-2 py-0.5 text-[11px] font-mono leading-none">` with an optional small `x` button when `onRemove` is set (button must `stopPropagation` + `preventDefault` so it works inside the card `<Link>`).

4. `components/labels/LabelPicker.tsx`: `LabelPicker({ meetingId, selected: Label[], open, onClose, anchorClassName? })`. Behavior:
   - Positioned dropdown (same visual language as the kebab menu in MeetingCardActions: `absolute mt-1 bg-card border border-line rounded-card shadow-pop py-1 min-w-[220px] z-20 text-[13px]`), click-outside and Escape close it. Every click inside `stopPropagation`s + `preventDefault`s so it can live inside the card `<Link>`.
   - Top: text input (autofocus) placeholder "Search or create label".
   - List: `useLabels()` filtered by the query (case-insensitive substring). Each row: colored dot, name, check mark when selected; clicking toggles via `useSetMeetingLabels` with the new full id set.
   - When the trimmed query is non-empty and no existing label name equals it (NOCASE), show a final row `Create "<query>"`; Enter also triggers it. It calls `useCreateLabel` then `useSetMeetingLabels` with selected ids + the new id, then clears the input.
   - Empty state when no labels exist and no query: "No labels yet. Type to create one."
   Keep this component under ~150 lines. `// ponytail:` comment where you simplify (e.g. no keyboard arrow navigation).

5. `components/labels/MeetingLabels.tsx`: `MeetingLabels({ meetingId, labels?: Label[], compact?: boolean })`. If `labels` is omitted, read them from `useMeeting(meetingId).data?.labels ?? []`. Renders chips (with `onRemove` -> `useSetMeetingLabels` minus that id) plus a trailing `+ Label` ghost button (`Tag` icon from lucide-react, `text-mut hover:text-ink text-[12px]`) that opens `LabelPicker`. Wrapper is a positioned `div`.

6. `library/MeetingCard.tsx`: under the meta line, when `meeting.labels.length > 0`, render `<div class="mt-1 flex flex-wrap gap-1">` of `LabelChip size="sm"` (no remove button on the card; removal happens in the picker). Keep the card row height stable when there are no labels.

7. `library/MeetingCardActions.tsx`: add a `Tag` icon button between the star and the kebab (same hover-reveal classes as the star button, `aria-label="Edit labels"`) that toggles a `LabelPicker` anchored in the same wrapper; accept a new `labels: Label[]` prop and pass it through from MeetingCard. Close the picker when the kebab opens and vice versa.

8. `library/Sidebar.tsx`: add a `Labels` section under the nav (`<div class="px-5 pt-5 pb-1 text-[11px] font-mono uppercase tracking-wider text-mut">Labels</div>`). For each label from `useLabels()`: a `NavLink to={`/label/${id}`}` row with the same active/inactive classes as "Starred", content = color dot (`w-2 h-2 rounded-pill` with `LABEL_COLORS[color].fg`) + truncated name + right-aligned mono count. On hover (`group`), reveal a `MoreHorizontal` kebab with a small menu: `Rename` (turns the row into an inline input; Enter commits via `useUpdateLabel`, Escape cancels), a row of six color swatches (click -> `useUpdateLabel({color})`), and `Delete` with the same inline `Delete?`/`Cancel` confirm pattern as MeetingCardActions (caption: "Removes the label from N meetings"). The kebab must `preventDefault` so the NavLink does not navigate. If there are no labels, render a single muted line "No labels yet" under the heading. Extract nothing into new files unless the Sidebar grows past ~250 lines; if it does, move the row into `components/library/SidebarLabelRow.tsx`.

9. Routing: `router.tsx` add `{ path: "/label/:labelId", element: <Library /> }`. `Library.tsx`: `const { labelId } = useParams()`; filter `meetings` by `m.labels.some(l => l.id === labelId)` when set; the Greeting count follows the filtered list like `starredOnly` does; empty state for a label filter is a muted line `No meetings with this label yet.`; if `labelId` is set but not present in `useLabels().data` (deleted), `<Navigate to="/" replace />`.

10. Mount points:
   - `routes/Meeting.tsx`: inside the `<header>` add a row after row 1 (before the MicDevicePicker row) rendering `<MeetingLabels meetingId={meetingId} />` when `meetingId` is set.
   - `routes/MeetingPost.tsx`: under the title/subline block in the header, render `<MeetingLabels meetingId={meetingId} compact />` when `meetingId` is set. It fetches via `useMeeting`, so no change to the raw-fetch hydration code is needed.

11. Tests (`LabelPicker.test.tsx`, vitest + testing-library, wrap in a `QueryClientProvider` like `MeetingCard.test.tsx` does; mock `fetch` or mock the hooks modules with `vi.mock` - follow whichever pattern `MeetingCardActions`/`MicDevicePicker.test.tsx` already use):
   - renders existing labels and marks selected ones checked;
   - typing a name with no match shows `Create "…"` and Enter calls create then set with the new id;
   - clicking an unselected label calls set with the added id; clicking a selected one calls set without it.
   Also add one Sidebar test asserting labels render with counts and link to `/label/:id` (extend an existing Sidebar/Library test file if one exists; otherwise a small `Sidebar.labels.test.tsx`).

12. Run `pnpm -C web typecheck && pnpm -C web test`. Fix any fixture breakage from the `labels` field.
  </action>
  <verify>cd web && pnpm typecheck && pnpm test 2>&1 | tail -15</verify>
  <done>Typecheck and all vitest suites pass; labels can be created/applied from the card, the live meeting header, and the post view; the sidebar lists labels with counts, filters via /label/:id, and supports rename/recolor/delete.</done>
</task>

</tasks>

<verification>
- `cargo test --workspace` green.
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `pnpm -C web typecheck && pnpm -C web test` green.
- Manual smoke (if a dev server is reachable): create a label from a card, see it in the sidebar with count 1, click it to filter, rename it, delete it.
</verification>

<success_criteria>
All must_haves truths hold; three atomic commits (db, server, web) on the current branch; SUMMARY.md written.
</success_criteria>

<output>
After completion, create `.planning/quick/260828-ogf-add-meeting-labels-granola-style-with-si/260828-ogf-SUMMARY.md` following the summary template.
</output>
