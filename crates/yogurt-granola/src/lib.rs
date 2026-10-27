//! Client and mapping for Granola's public REST API (`https://public-api.granola.ai/v1`).
//! The key goes only in the `Authorization` header and never into an error, a log line, or a type that derives `Debug`.

mod client;
mod map;
mod model;

pub use client::{Client, Config, Error};
pub use map::{map_note, SOURCE};
pub use model::{Folder, Note, NoteSummary, Speaker, TranscriptLine, User};
