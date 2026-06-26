//! Phase 5 (Plan 05-03) HTTP API module — currently just the `settings`
//! surface (`/api/settings*`). Phase 4's `enhance` and `meetings` handlers
//! remain at their existing `crates/yogurt-server/src/{enhance,routes}.rs`
//! locations; this module is the entry point for *new* API surfaces added
//! after Phase 4.

pub mod chat;
// Phase 7 (Plan 07-01): Library REST surface — CRUD over the SQLite-backed
// `MeetingRepo`. Coexists with the Phase 3 `POST /api/meetings`
// (`routes::create_meeting`) which still creates the in-memory streaming
// `Registry` entry; Phase 7's handlers operate on the persisted directory
// instead.
pub mod meetings;
pub mod settings;
// Phase 8 (Plan 08-03): whisper.cpp model management REST surface.
// Routes mount under `/api/stt/*` and spawn background download tasks
// that emit `stt_model_download_*` events on the app-wide `/ws` channel.
pub mod stt_models;
