//! LLM client resolution for the enhance handler.
//!
//! There is deliberately no automatic fallback from an unreachable `http` provider to a
//! local CLI. Silently rerouting a meeting's content to an unvetted backend because of a
//! network hiccup is a change the user must opt into by picking the CLI as the active provider.

use std::sync::Arc;

pub use yogurt_llm::OpenAiCompatClient;

/// Shared by enhance and chat so they always use the same model.
///
/// Priority:
///   1. `state.llm_override`, test injection only.
///   2. `YOGURT_LLM_*` env vars.
///   3. Active provider row. An unreadable key or a CLI missing from `$PATH` is a hard `Err`.
///   4. `MockLlm` when nothing is configured, so first-run users still see the hero flow.
pub async fn resolve(
    state: &crate::state::AppState,
    purpose: &'static str,
    meeting_id: Option<String>,
) -> anyhow::Result<std::sync::Arc<dyn yogurt_llm::LlmClient>> {
    match resolve_configured(state, purpose, meeting_id).await? {
        Some(client) => Ok(client),
        None => {
            tracing::warn!("no LLM provider configured; falling back to MockLlm");
            Ok(Arc::new(crate::llm_mock::MockLlm))
        }
    }
}

/// Steps 1 to 3 of [`resolve`], with `Ok(None)` where it would fall back to the mock.
/// `purpose` and `meeting_id` label the client's `llm_call` log lines.
pub async fn resolve_configured(
    state: &crate::state::AppState,
    purpose: &'static str,
    meeting_id: Option<String>,
) -> anyhow::Result<Option<Arc<dyn yogurt_llm::LlmClient>>> {
    let inner = if let Some(overridden) = &state.llm_override {
        overridden.clone()
    } else if let Some(c) = from_env() {
        Arc::new(c)
    } else {
        match from_active_provider(&state.db, state.keys.clone()).await? {
            Some(c) => c,
            None => return Ok(None),
        }
    };
    Ok(Some(Arc::new(Logged {
        inner,
        purpose,
        meeting_id,
    })))
}

/// Writes one `llm_call` line per request, so a log shows how often each model was called for a meeting and how each call ended.
struct Logged {
    inner: Arc<dyn yogurt_llm::LlmClient>,
    purpose: &'static str,
    meeting_id: Option<String>,
}

impl Logged {
    fn call(&self) -> Call {
        Call {
            purpose: self.purpose,
            meeting_id: self.meeting_id.clone(),
            model: self.inner.model_name(),
            started: std::time::Instant::now(),
            logged: false,
        }
    }
}

struct Call {
    purpose: &'static str,
    meeting_id: Option<String>,
    model: String,
    started: std::time::Instant,
    logged: bool,
}

impl Call {
    fn finish(&mut self, outcome: &str, error: Option<&anyhow::Error>) {
        if std::mem::replace(&mut self.logged, true) {
            return;
        }
        tracing::info!(
            event = "llm_call",
            purpose = self.purpose,
            meeting_id = self.meeting_id.as_deref(),
            model = %self.model,
            elapsed_ms = self.started.elapsed().as_millis() as u64,
            outcome,
            error = error.map(|e| format!("{e:#}")).as_deref(),
            "llm call"
        );
    }
}

/// A caller that times out, a client that disconnects, or a stream with no `done` chunk drops the call before it finished.
impl Drop for Call {
    fn drop(&mut self) {
        self.finish("unfinished", None);
    }
}

#[async_trait::async_trait]
impl yogurt_llm::LlmClient for Logged {
    async fn complete(
        &self,
        req: yogurt_llm::ChatRequest,
    ) -> anyhow::Result<yogurt_llm::ChatResponse> {
        let mut call = self.call();
        let res = self.inner.complete(req).await;
        match &res {
            Ok(_) => call.finish("ok", None),
            Err(e) => call.finish("error", Some(e)),
        }
        res
    }

    async fn stream(
        &self,
        req: yogurt_llm::ChatRequest,
    ) -> anyhow::Result<
        futures_util::stream::BoxStream<'static, anyhow::Result<yogurt_llm::ChatChunk>>,
    > {
        use futures_util::StreamExt;
        let mut call = self.call();
        let stream = self
            .inner
            .stream(req)
            .await
            .inspect_err(|e| call.finish("error", Some(e)))?;
        Ok(stream
            .inspect(move |chunk| match chunk {
                Ok(c) if c.done => call.finish("ok", None),
                Err(e) => call.finish("error", Some(e)),
                Ok(_) => {}
            })
            .boxed())
    }

    fn model_name(&self) -> String {
        self.inner.model_name()
    }

    fn response_timeout(&self) -> std::time::Duration {
        self.inner.response_timeout()
    }
}

/// Developer override. `None` unless all of `YOGURT_LLM_BASE_URL`, `YOGURT_LLM_API_KEY`
/// and `YOGURT_LLM_MODEL` are set.
pub fn from_env() -> Option<OpenAiCompatClient> {
    let base_url = std::env::var("YOGURT_LLM_BASE_URL").ok()?;
    let api_key = std::env::var("YOGURT_LLM_API_KEY").ok()?;
    let model = std::env::var("YOGURT_LLM_MODEL").ok()?;
    Some(OpenAiCompatClient::new(base_url, api_key, model))
}

/// `Ok(None)` means no active provider, so the caller falls back to mock.
/// `Err` means the active provider is unusable (unreadable key, CLI not on `$PATH`) and the
/// caller must surface it (502) rather than fall back.
async fn from_active_provider(
    db: &yogurt_db::Db,
    keys: std::sync::Arc<dyn yogurt_db::keys::ApiKeyStore>,
) -> anyhow::Result<Option<Arc<dyn yogurt_llm::LlmClient>>> {
    let Some(provider) = yogurt_db::providers::active(db)? else {
        return Ok(None);
    };
    if provider.adapter == yogurt_db::providers::adapter::CLI {
        let program = yogurt_llm::CliProgram::parse(&provider.model).ok_or_else(|| {
            anyhow::anyhow!(
                "provider '{}' ({}) has an unrecognized CLI program '{}'",
                provider.name,
                provider.id,
                provider.model
            )
        })?;
        let cli_model = (!provider.cli_model.is_empty()).then_some(provider.cli_model.clone());
        let client = yogurt_llm::CliClient::locate(program, cli_model).map_err(|e| {
            anyhow::anyhow!(
                "provider '{}' ({}) is active but its CLI could not be found: {e} \
                 - install it and make sure it's on $PATH",
                provider.name,
                provider.id
            )
        })?;
        return Ok(Some(Arc::new(client)));
    }
    let key = keys.get(&provider.id)?;
    // The error names the provider, never the key.
    match key {
        Some(key) => Ok(Some(Arc::new(OpenAiCompatClient::new(
            provider.base_url,
            key,
            provider.model,
        )))),
        None => anyhow::bail!(
            "provider '{}' ({}) is active but its API key could not be read \
             from the key store - re-enter it in Settings",
            provider.name,
            provider.id
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use yogurt_db::keys::{ApiKeyStore, MemoryKeyStore};
    use yogurt_db::providers::{insert, set_active, NewProvider};
    use yogurt_db::Db;

    fn fixture() -> (Db, Arc<dyn ApiKeyStore>) {
        let db = Db::open_in_memory().unwrap();
        let keys: Arc<dyn ApiKeyStore> = Arc::new(MemoryKeyStore::default());
        (db, keys)
    }

    fn insert_active_provider(db: &Db) -> String {
        let id = insert(
            db,
            NewProvider {
                name: "Minimax".to_string(),
                base_url: "https://api.minimax.io/v1".to_string(),
                model: "MiniMax-Text-01".to_string(),
                adapter: yogurt_db::providers::adapter::HTTP.to_string(),
                cli_model: String::new(),
            },
        )
        .unwrap();
        set_active(db, &id).unwrap();
        id
    }

    fn insert_active_cli_provider(db: &Db, model: &str) -> String {
        let id = insert(
            db,
            NewProvider {
                name: "Claude Code (local CLI)".to_string(),
                base_url: String::new(),
                model: model.to_string(),
                adapter: yogurt_db::providers::adapter::CLI.to_string(),
                cli_model: String::new(),
            },
        )
        .unwrap();
        set_active(db, &id).unwrap();
        id
    }

    #[tokio::test]
    async fn no_provider_returns_none() {
        let (db, keys) = fixture();
        let got = from_active_provider(&db, keys).await.unwrap();
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn provider_with_key_returns_client() {
        let (db, keys) = fixture();
        let id = insert_active_provider(&db);
        keys.set(&id, "sk-test").unwrap();
        // `Arc<dyn LlmClient>` isn't `Debug`, so `unwrap` is unavailable.
        let client = match from_active_provider(&db, keys).await {
            Ok(Some(c)) => c,
            other => panic!("expected Ok(Some(_)), got {}", other.is_ok()),
        };
        assert_eq!(client.model_name(), "MiniMax-Text-01");
    }

    #[tokio::test]
    async fn provider_without_key_errors_naming_provider() {
        let (db, keys) = fixture();
        insert_active_provider(&db);
        let err = match from_active_provider(&db, keys).await {
            Err(e) => e,
            Ok(_) => panic!("expected Err when key is missing"),
        };
        let msg = err.to_string();
        assert!(msg.contains("Minimax"), "error should name provider: {msg}");
        assert!(msg.to_lowercase().contains("key"), "actionable msg: {msg}");
    }

    /// An unrecognized program never resolves, whatever CLIs the machine has installed.
    #[tokio::test]
    async fn cli_provider_with_unrecognized_program_errors_naming_provider() {
        let (db, keys) = fixture();
        insert_active_cli_provider(&db, "not-a-real-cli-program");
        let err = match from_active_provider(&db, keys).await {
            Err(e) => e,
            Ok(_) => panic!("expected Err for an unrecognized CLI program"),
        };
        let msg = err.to_string();
        assert!(
            msg.contains("Claude Code (local CLI)"),
            "error should name the provider: {msg}"
        );
        assert!(
            msg.contains("not-a-real-cli-program"),
            "error should name the bad program value: {msg}"
        );
    }

    /// Resolves when `claude` is on `$PATH` and errors otherwise, never `Ok(None)`.
    /// Both branches are asserted so it passes on CI and on a machine with `claude`.
    #[tokio::test]
    async fn cli_provider_with_recognized_program_resolves_or_errors_naming_provider() {
        let (db, keys) = fixture();
        insert_active_cli_provider(&db, "claude");
        match from_active_provider(&db, keys).await {
            Ok(Some(client)) => {
                assert_eq!(client.model_name(), "cli:claude");
            }
            Ok(None) => panic!("expected Some(_) or an Err, got None"),
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("Claude Code (local CLI)"),
                    "error should name the provider: {msg}"
                );
            }
        }
    }

    struct Fails;

    #[async_trait::async_trait]
    impl yogurt_llm::LlmClient for Fails {
        async fn complete(
            &self,
            _: yogurt_llm::ChatRequest,
        ) -> anyhow::Result<yogurt_llm::ChatResponse> {
            Err(anyhow::anyhow!("deadline has elapsed").context("error sending request"))
        }
        async fn stream(
            &self,
            _: yogurt_llm::ChatRequest,
        ) -> anyhow::Result<
            futures_util::stream::BoxStream<'static, anyhow::Result<yogurt_llm::ChatChunk>>,
        > {
            anyhow::bail!("unused")
        }
        fn model_name(&self) -> String {
            "fails".into()
        }
    }

    #[derive(Clone, Default)]
    struct Lines(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Lines {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn every_call_logs_one_llm_call_line_with_how_it_ended() {
        use futures_util::StreamExt;
        use yogurt_llm::{ChatMessage, ChatRequest, LlmClient};
        let out = Lines::default();
        let writer = out.clone();
        let _guard = tracing::subscriber::set_default(
            tracing_subscriber::fmt()
                .with_writer(move || writer.clone())
                .with_ansi(false)
                .finish(),
        );
        let req = || ChatRequest {
            messages: vec![ChatMessage::user("hi")],
            stream: false,
        };
        let logged = |inner: Arc<dyn LlmClient>| Logged {
            inner,
            purpose: "recap",
            meeting_id: Some("m1".into()),
        };

        let mock = logged(Arc::new(crate::llm_mock::MockLlm));
        mock.complete(req()).await.unwrap();
        let mut s = mock.stream(req()).await.unwrap();
        while s.next().await.is_some() {}
        drop(s);
        drop(mock.stream(req()).await.unwrap());
        assert!(logged(Arc::new(Fails)).complete(req()).await.is_err());

        let text = String::from_utf8(out.0.lock().unwrap().clone()).unwrap();
        let lines: Vec<_> = text.lines().filter(|l| l.contains("llm_call")).collect();
        assert_eq!(lines.len(), 4, "{text}");
        assert!(
            lines
                .iter()
                .all(|l| l.contains("purpose=\"recap\"") && l.contains("meeting_id=\"m1\"")),
            "{text}"
        );
        assert!(lines[0].contains("outcome=\"ok\""), "{}", lines[0]);
        assert!(lines[1].contains("outcome=\"ok\""), "{}", lines[1]);
        assert!(lines[2].contains("outcome=\"unfinished\""), "{}", lines[2]);
        assert!(
            lines[3].contains("outcome=\"error\"")
                && lines[3].contains("error sending request: deadline has elapsed"),
            "{}",
            lines[3]
        );
    }
}
