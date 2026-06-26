//! SSE streaming for `OpenAiCompatClient` (Plan 05-01 Task 2).
//!
//! Implementation lands in Task 2 of the same plan — this placeholder lets
//! `lib.rs::stream()` compile and exposes a clear surface error if a caller
//! accidentally invokes streaming before the real impl is in place.

use crate::{ChatChunk, ChatRequest, OpenAiCompatClient};
use anyhow::Result;
use futures_util::stream::BoxStream;

pub(crate) async fn stream(
    _client: &OpenAiCompatClient,
    _req: ChatRequest,
) -> Result<BoxStream<'static, Result<ChatChunk>>> {
    anyhow::bail!("streaming not yet implemented (Plan 05-01 Task 2)")
}
