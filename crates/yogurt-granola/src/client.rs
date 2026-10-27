use crate::model::{Note, NoteSummary, TranscriptLine};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Granola rejected the API key")]
    Unauthorized,
    #[error("note not found")]
    NotFound,
    #[error("transcript too large to return inline")]
    TooLarge,
    /// A 429, a 5xx, or a dropped connection that kept failing through every retry.
    #[error("Granola kept failing after retries: {0}")]
    Exhausted(String),
    #[error("Granola answered HTTP {0}")]
    Http(u16),
    #[error("unreadable Granola response: {0}")]
    Decode(String),
}

#[derive(Debug, Clone)]
pub struct Config {
    pub base_url: String,
    /// Spacing between requests. 250 ms is 4 a second, under Granola's 5 a second sustained limit.
    pub min_interval: Duration,
    /// First retry delay. It doubles each retry unless the response carries `Retry-After`.
    pub backoff: Duration,
    /// Upper bound for one request, so a dead socket surfaces as a retryable network error instead of hanging.
    pub request_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            base_url: "https://public-api.granola.ai/v1".into(),
            min_interval: Duration::from_millis(250),
            backoff: Duration::from_secs(1),
            request_timeout: Duration::from_secs(60),
        }
    }
}

const RETRIES: u32 = 5;
const MAX_BACKOFF: Duration = Duration::from_secs(60);
const LIST_PAGE: u32 = 30;
const TRANSCRIPT_PAGE: u32 = 100;

/// Deliberately not `Debug`, because it holds the API key.
pub struct Client {
    http: reqwest::Client,
    key: String,
    cfg: Config,
    next_slot: Mutex<Instant>,
}

#[derive(Deserialize)]
struct Page<T> {
    #[serde(alias = "notes", alias = "transcript")]
    items: Vec<T>,
    #[serde(default, rename = "hasMore")]
    has_more: bool,
    #[serde(default)]
    cursor: Option<String>,
}

impl Client {
    pub fn new(key: &str, cfg: Config) -> Self {
        Self {
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(cfg.request_timeout)
                .build()
                .expect("static reqwest client config"),
            key: key.trim().to_string(),
            cfg,
            next_slot: Mutex::new(Instant::now()),
        }
    }

    /// One request for the first page. Proves the key works.
    pub async fn check(&self) -> Result<(), Error> {
        self.page::<NoteSummary>("/notes?page_size=1")
            .await
            .map(|_| ())
    }

    /// Every note the key can see, newest first, following the cursor to the end.
    pub async fn list_notes(&self) -> Result<Vec<NoteSummary>, Error> {
        self.all(&format!("/notes?page_size={LIST_PAGE}")).await
    }

    /// The note with its transcript. A 413 on the inline transcript falls back to the paged endpoint.
    pub async fn get_note(&self, id: &str) -> Result<Note, Error> {
        match self.json(&format!("/notes/{id}?include=transcript")).await {
            Err(Error::TooLarge) => {
                let mut note: Note = self.json(&format!("/notes/{id}")).await?;
                let lines: Vec<TranscriptLine> = self
                    .all(&format!(
                        "/notes/{id}/transcript?page_size={TRANSCRIPT_PAGE}"
                    ))
                    .await?;
                note.transcript = Some(lines);
                Ok(note)
            }
            other => other,
        }
    }

    async fn all<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>, Error> {
        let mut out = Vec::new();
        let mut cursor: Option<String> = None;
        loop {
            let url = match &cursor {
                Some(c) => format!("{path}&cursor={}", encode(c)),
                None => path.to_string(),
            };
            let page: Page<T> = self.page(&url).await?;
            out.extend(page.items);
            match (page.has_more, page.cursor) {
                (true, Some(c)) => cursor = Some(c),
                _ => return Ok(out),
            }
        }
    }

    async fn page<T: DeserializeOwned>(&self, path: &str) -> Result<Page<T>, Error> {
        self.json(path).await
    }

    async fn json<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        let resp = self.get(path).await?;
        resp.json()
            .await
            .map_err(|e| Error::Decode(e.without_url().to_string()))
    }

    /// Paces every request, and retries a 429, a 5xx, or a network error with doubling delays.
    async fn get(&self, path: &str) -> Result<reqwest::Response, Error> {
        let url = format!("{}{path}", self.cfg.base_url);
        let mut delay = self.cfg.backoff;
        let mut last = String::new();
        for attempt in 0..=RETRIES {
            self.pace().await;
            let sent = self.http.get(&url).bearer_auth(&self.key).send().await;
            let mut wait = delay;
            match sent {
                Ok(resp) => {
                    let status = resp.status();
                    match status.as_u16() {
                        200..=299 => return Ok(resp),
                        401 => return Err(Error::Unauthorized),
                        404 => return Err(Error::NotFound),
                        413 => return Err(Error::TooLarge),
                        429 | 500..=599 => {
                            last = format!("HTTP {}", status.as_u16());
                            if let Some(secs) = resp
                                .headers()
                                .get(reqwest::header::RETRY_AFTER)
                                .and_then(|v| v.to_str().ok())
                                .and_then(|v| v.trim().parse::<u64>().ok())
                            {
                                wait = wait.max(Duration::from_secs(secs));
                            }
                        }
                        code => return Err(Error::Http(code)),
                    }
                }
                Err(e) => last = e.without_url().to_string(),
            }
            if attempt < RETRIES {
                tokio::time::sleep(wait.min(MAX_BACKOFF)).await;
                delay = (delay * 2).min(MAX_BACKOFF);
            }
        }
        Err(Error::Exhausted(last))
    }

    async fn pace(&self) {
        let mut next = self.next_slot.lock().await;
        let now = Instant::now();
        if *next > now {
            tokio::time::sleep_until(*next).await;
        }
        *next = (*next).max(now) + self.cfg.min_interval;
    }
}

/// Cursors are opaque base64 and can hold `+`, `/`, and `=`.
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(server: &MockServer) -> Client {
        Client::new(
            "grn_test_key",
            Config {
                base_url: server.uri(),
                min_interval: Duration::ZERO,
                backoff: Duration::from_millis(1),
                request_timeout: Duration::from_secs(5),
            },
        )
    }

    fn note_json(id: &str) -> serde_json::Value {
        json!({ "id": id, "title": "t", "owner": { "name": null, "email": "o@example.com" } })
    }

    #[tokio::test]
    async fn list_follows_the_cursor_and_sends_the_bearer_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/notes"))
            .and(header("authorization", "Bearer grn_test_key"))
            .and(query_param("page_size", "30"))
            .and(query_param_is_missing("cursor"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "notes": [note_json("not_1")], "hasMore": true, "cursor": "a+b/c=" }),
            ))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/notes"))
            .and(query_param("cursor", "a+b/c="))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "notes": [note_json("not_2")], "hasMore": false, "cursor": null }),
            ))
            .expect(1)
            .mount(&server)
            .await;
        let ids: Vec<_> = client(&server)
            .list_notes()
            .await
            .unwrap()
            .into_iter()
            .map(|n| n.id)
            .collect();
        assert_eq!(ids, ["not_1", "not_2"]);
    }

    #[tokio::test]
    async fn a_429_retries_and_honors_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/notes/not_1"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/notes/not_1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(note_json("not_1")))
            .mount(&server)
            .await;
        assert_eq!(client(&server).get_note("not_1").await.unwrap().id, "not_1");
    }

    #[tokio::test]
    async fn retry_after_never_shortens_the_backoff() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
            .mount(&server)
            .await;
        let c = Client::new(
            "k",
            Config {
                base_url: server.uri(),
                min_interval: Duration::ZERO,
                backoff: Duration::from_millis(20),
                request_timeout: Duration::from_secs(5),
            },
        );
        let t = std::time::Instant::now();
        assert!(matches!(c.get_note("n").await, Err(Error::Exhausted(_))));
        // 20 + 40 + 80 + 160 + 320 ms of backoff across five retries.
        assert!(
            t.elapsed() >= Duration::from_millis(600),
            "{:?}",
            t.elapsed()
        );
    }

    #[tokio::test]
    async fn a_hung_request_times_out_and_is_retried() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(500)))
            .expect(6)
            .mount(&server)
            .await;
        let c = Client::new(
            "k",
            Config {
                base_url: server.uri(),
                min_interval: Duration::ZERO,
                backoff: Duration::from_millis(1),
                request_timeout: Duration::from_millis(30),
            },
        );
        assert!(matches!(c.get_note("n").await, Err(Error::Exhausted(_))));
    }

    #[tokio::test]
    async fn retries_stop_after_five_and_report_exhaustion() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .expect(6)
            .mount(&server)
            .await;
        let err = client(&server).get_note("not_1").await.unwrap_err();
        assert!(matches!(err, Error::Exhausted(_)), "{err}");
    }

    #[tokio::test]
    async fn auth_and_missing_notes_do_not_retry() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/notes/gone"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/notes/denied"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        let c = client(&server);
        assert!(matches!(c.get_note("gone").await, Err(Error::NotFound)));
        let err = c.get_note("denied").await.unwrap_err();
        assert!(matches!(err, Error::Unauthorized));
        assert!(!err.to_string().contains("grn_test_key"));
    }

    #[tokio::test]
    async fn a_413_falls_back_to_the_paged_transcript() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/notes/not_big"))
            .and(query_param("include", "transcript"))
            .respond_with(ResponseTemplate::new(413))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/notes/not_big"))
            .and(query_param_is_missing("include"))
            .respond_with(ResponseTemplate::new(200).set_body_json(note_json("not_big")))
            .expect(1)
            .mount(&server)
            .await;
        let line = |text: &str| json!({ "speaker": { "source": "speaker" }, "text": text, "start_time": "2026-01-02T09:00:00Z", "end_time": "2026-01-02T09:00:01Z" });
        Mock::given(method("GET"))
            .and(path("/notes/not_big/transcript"))
            .and(query_param("page_size", "100"))
            .and(query_param_is_missing("cursor"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "transcript": [line("one")], "hasMore": true, "cursor": "next" }),
            ))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/notes/not_big/transcript"))
            .and(query_param("cursor", "next"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({ "transcript": [line("two")], "hasMore": false, "cursor": null }),
            ))
            .expect(1)
            .mount(&server)
            .await;
        let note = client(&server).get_note("not_big").await.unwrap();
        let texts: Vec<_> = note
            .transcript
            .unwrap()
            .into_iter()
            .map(|l| l.text)
            .collect();
        assert_eq!(texts, ["one", "two"]);
    }

    #[tokio::test]
    async fn requests_are_spaced_by_the_configured_interval() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_json(note_json("not_1")))
            .mount(&server)
            .await;
        let c = Client::new(
            "k",
            Config {
                base_url: server.uri(),
                min_interval: Duration::from_millis(40),
                backoff: Duration::from_millis(1),
                request_timeout: Duration::from_secs(5),
            },
        );
        let t = std::time::Instant::now();
        for _ in 0..4 {
            c.get_note("not_1").await.unwrap();
        }
        assert!(
            t.elapsed() >= Duration::from_millis(110),
            "{:?}",
            t.elapsed()
        );
    }
}
