use serde::Serialize;

/// Context passed into `enhance.md`.
///
/// `notes` is the raw user-authored markdown (preserve verbatim).
/// `transcript` is a pre-serialized JSON array of transcript segments
/// (the LLM sees the JSON directly so it can mine timestamps).
#[derive(Serialize, Debug)]
pub struct EnhanceCtx<'a> {
    pub notes: &'a str,
    pub transcript: &'a str,
}
