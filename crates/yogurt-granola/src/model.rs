//! The slice of Granola's response shapes the importer reads. Unknown fields are ignored.

use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct User {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub email: String,
}

/// One entry of `GET /v1/notes`.
#[derive(Debug, Clone, Deserialize)]
pub struct NoteSummary {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub owner: User,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Folder {
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Speaker {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub attribution: Option<String>,
    #[serde(default)]
    pub diarization_label: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptLine {
    #[serde(default)]
    pub speaker: Speaker,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub start_time: String,
    #[serde(default)]
    pub end_time: String,
}

/// `GET /v1/notes/{id}`, with `transcript` filled by `include=transcript` or the paged endpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct Note {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub owner: User,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub web_url: Option<String>,
    #[serde(default)]
    pub folder_membership: Vec<Folder>,
    #[serde(default)]
    pub summary_text: Option<String>,
    #[serde(default)]
    pub summary_markdown: Option<String>,
    #[serde(default)]
    pub private_notes_markdown: Option<String>,
    #[serde(default)]
    pub transcript: Option<Vec<TranscriptLine>>,
}
