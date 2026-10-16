use serde::Serialize;

/// `transcript` is a pre-serialized JSON array so the model can mine timestamps.
/// `template` forces a note format by id, and `None` lets the model pick and name it on the first line.
#[derive(Serialize, Debug)]
pub struct EnhanceCtx<'a> {
    pub notes: &'a str,
    pub transcript: &'a str,
    #[serde(skip)]
    pub template: Option<&'a str>,
}
