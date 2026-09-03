use serde::Serialize;

/// Context passed into `enhance.md`.
///
/// `notes` is the raw user-authored markdown (preserve verbatim).
/// `transcript` is a pre-serialized JSON array of transcript segments
/// (the LLM sees the JSON directly so it can mine timestamps).
/// `template` is the id of the note format to force (see
/// [`crate::TEMPLATE_IDS`]); `None` asks the model to pick the best fit
/// itself and name it on the output's first line.
#[derive(Serialize, Debug)]
pub struct EnhanceCtx<'a> {
    pub notes: &'a str,
    pub transcript: &'a str,
    #[serde(skip)]
    pub template: Option<&'a str>,
}
