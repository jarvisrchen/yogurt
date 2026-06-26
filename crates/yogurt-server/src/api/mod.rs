//! Phase 5 (Plan 05-03) HTTP API module — currently just the `settings`
//! surface (`/api/settings*`). Phase 4's `enhance` and `meetings` handlers
//! remain at their existing `crates/yogurt-server/src/{enhance,routes}.rs`
//! locations; this module is the entry point for *new* API surfaces added
//! after Phase 4.

pub mod settings;
