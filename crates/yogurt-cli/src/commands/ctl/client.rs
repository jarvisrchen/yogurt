//! HTTP client and server discovery for `yogurt ctl`.
//! Discovery precedence: `--port`, then `$YOGURT_PORT`, then a health scan of the port range. Every failure collapses to a `CtlError`.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

/// `just dev` / `port-guard.sh` widen a busy default port by up to 20, so a scan covers the same window.
pub const PORT_RANGE: std::ops::RangeInclusive<u16> = 7878..=7898;

/// A single probe result. `version` and `mode` are absent from older binaries.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Instance {
    pub port: u16,
    pub version: String,
    pub mode: String,
}

/// Every business-logic failure `ctl` can hit, already carrying its own `help:` line.
#[derive(Debug)]
pub enum CtlError {
    /// No server answered at this specific port (`Some`), or the scan of
    /// [`PORT_RANGE`] found nothing (`None`).
    NoServer(Option<u16>),
    /// More than one instance answered and nothing disambiguated which one
    /// to talk to.
    Ambiguous(Vec<u16>),
    /// The server answered with a non-2xx status; `message` is its
    /// `{"error": "..."}` body (or the raw text if it wasn't JSON).
    Server(String),
    /// Like `Server`, for an answer that carries a stable `code` next to its message. A caller branches on the code and never on the English text.
    Coded { code: String, message: String },
    /// Anything else: a bad local path, a malformed response, a dropped connection.
    Local { message: String, help: String },
}

impl CtlError {
    pub fn local(message: impl Into<String>, help: impl Into<String>) -> Self {
        CtlError::Local {
            message: message.into(),
            help: help.into(),
        }
    }

    pub fn message_and_help(&self) -> (String, String) {
        match self {
            CtlError::NoServer(Some(port)) => (
                format!("no yogurt server answering on port {port}"),
                format!(
                    "run `yogurt start --port {port}`, or drop --port to scan {}-{}",
                    PORT_RANGE.start(),
                    PORT_RANGE.end()
                ),
            ),
            CtlError::NoServer(None) => (
                format!(
                    "no yogurt server found (scanned ports {}-{})",
                    PORT_RANGE.start(),
                    PORT_RANGE.end()
                ),
                "run `yogurt start`".to_string(),
            ),
            CtlError::Ambiguous(ports) => {
                let list = ports
                    .iter()
                    .map(u16::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                (
                    format!("multiple yogurt instances found (ports {list})"),
                    "pass --port <port>".to_string(),
                )
            }
            CtlError::Server(message) | CtlError::Coded { message, .. } => {
                (message.clone(), "check `yogurt ctl status`".to_string())
            }
            CtlError::Local { message, help } => (message.clone(), help.clone()),
        }
    }
}

fn env_port() -> Option<u16> {
    std::env::var("YOGURT_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
}

async fn probe_health(http: &reqwest::Client, port: u16) -> Option<Instance> {
    let url = format!("http://127.0.0.1:{port}/api/health");
    let resp = http
        .get(url)
        .timeout(Duration::from_millis(400))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let body: serde_json::Value = resp.json().await.ok()?;
    if body.get("status").and_then(|v| v.as_str()) != Some("ok") {
        return None;
    }
    Some(Instance {
        port,
        version: body
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
        mode: body
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string(),
    })
}

async fn scan(http: &reqwest::Client) -> Vec<Instance> {
    let mut set = tokio::task::JoinSet::new();
    for port in PORT_RANGE {
        let http = http.clone();
        set.spawn(async move { probe_health(&http, port).await });
    }
    let mut found = Vec::new();
    while let Some(res) = set.join_next().await {
        if let Ok(Some(inst)) = res {
            found.push(inst);
        }
    }
    found.sort_by_key(|i| i.port);
    found
}

/// An ambiguous scan is not an error for `ctl status`, it is the answer.
pub enum StatusTarget {
    /// `--port` / `$YOGURT_PORT` named a single port; `None` if it didn't
    /// answer.
    Explicit(u16, Option<Instance>),
    /// No port was named -- every instance the scan found (0, 1, or more).
    Scanned(Vec<Instance>),
}

pub async fn discover_for_status(http: &reqwest::Client, explicit: Option<u16>) -> StatusTarget {
    if let Some(port) = port_precedence(explicit, env_port()) {
        return StatusTarget::Explicit(port, probe_health(http, port).await);
    }
    StatusTarget::Scanned(scan(http).await)
}

/// Discovery for every command except `status`.
pub async fn resolve(http: &reqwest::Client, explicit: Option<u16>) -> Result<Instance, CtlError> {
    if let Some(port) = port_precedence(explicit, env_port()) {
        return probe_health(http, port)
            .await
            .ok_or(CtlError::NoServer(Some(port)));
    }
    resolve_from_scan(scan(http).await)
}

/// Pure so it is testable without touching the process env.
fn port_precedence(flag: Option<u16>, env: Option<u16>) -> Option<u16> {
    flag.or(env)
}

/// Pure over the scan result so it is testable without a network.
fn resolve_from_scan(found: Vec<Instance>) -> Result<Instance, CtlError> {
    match found.len() {
        0 => Err(CtlError::NoServer(None)),
        1 => Ok(found.into_iter().next().expect("len == 1")),
        _ => Err(CtlError::Ambiguous(
            found.into_iter().map(|i| i.port).collect(),
        )),
    }
}

/// `$YOGURT_DATA_DIR` when set, else `~/.yogurt`. `ctl` has no `--data-dir` flag.
pub(crate) fn yogurt_home() -> Result<PathBuf, CtlError> {
    let env = std::env::var_os("YOGURT_DATA_DIR").map(PathBuf::from);
    let home = directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf());
    crate::data_dir::root(env, home)
        .ok_or_else(|| CtlError::local("could not resolve home directory", "check $HOME is set"))
}

/// No `ctl` subcommand ever prints the token value.
fn read_session_token() -> Result<String, CtlError> {
    let path = crate::data_dir::session_token_path(&yogurt_home()?);
    std::fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .map_err(|_| {
            CtlError::local(
                format!("could not read session token at {}", path.display()),
                "run `yogurt start` first",
            )
        })
}

pub struct Client {
    http: reqwest::Client,
    pub port: u16,
    token: String,
}

impl Client {
    pub fn new(http: reqwest::Client, port: u16, token: String) -> Self {
        Self { http, port, token }
    }

    /// Resolve a target server and load the session token. Used by every subcommand except `status`, `detect`, and `windows`.
    pub async fn discover(explicit_port: Option<u16>) -> Result<Self, CtlError> {
        let http = reqwest::Client::new();
        let instance = resolve(&http, explicit_port).await?;
        let token = read_session_token()?;
        Ok(Self::new(http, instance.port, token))
    }

    /// Skips discovery; used when a meeting URL already named the port.
    pub fn at_port(port: u16) -> Result<Self, CtlError> {
        let token = read_session_token()?;
        Ok(Self::new(reqwest::Client::new(), port, token))
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{path}", self.port)
    }

    fn conn_error(&self, e: reqwest::Error) -> CtlError {
        CtlError::local(
            format!("could not reach yogurt at 127.0.0.1:{}: {e}", self.port),
            "check the port is correct and the server is still running",
        )
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, CtlError> {
        let resp = self
            .http
            .get(self.url(path))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| self.conn_error(e))?;
        Self::handle_json(resp).await
    }

    pub async fn get_text(&self, path: &str) -> Result<String, CtlError> {
        let resp = self
            .http
            .get(self.url(path))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| self.conn_error(e))?;
        if resp.status().is_success() {
            resp.text().await.map_err(|e| {
                CtlError::local(
                    format!("bad response body: {e}"),
                    "check `yogurt ctl status`",
                )
            })
        } else {
            Err(CtlError::Server(server_error_message(resp).await))
        }
    }

    pub async fn post<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, CtlError> {
        let resp = self
            .http
            .post(self.url(path))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .map_err(|e| self.conn_error(e))?;
        Self::handle_json(resp).await
    }

    pub async fn post_empty<T: DeserializeOwned>(&self, path: &str) -> Result<T, CtlError> {
        let resp = self
            .http
            .post(self.url(path))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| self.conn_error(e))?;
        Self::handle_json(resp).await
    }

    /// For endpoints that answer with an empty body (e.g. the 202 model download start), where `post_empty`'s `resp.json()` would fail.
    pub async fn post_no_body(&self, path: &str) -> Result<(), CtlError> {
        let resp = self
            .http
            .post(self.url(path))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| self.conn_error(e))?;
        Self::handle_status(resp).await
    }

    pub async fn patch<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, CtlError> {
        let resp = self
            .http
            .patch(self.url(path))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .map_err(|e| self.conn_error(e))?;
        Self::handle_json(resp).await
    }

    pub async fn put<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, CtlError> {
        let resp = self
            .http
            .put(self.url(path))
            .bearer_auth(&self.token)
            .json(body)
            .send()
            .await
            .map_err(|e| self.conn_error(e))?;
        Self::handle_json(resp).await
    }

    pub async fn delete<T: DeserializeOwned>(&self, path: &str) -> Result<T, CtlError> {
        let resp = self
            .http
            .delete(self.url(path))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| self.conn_error(e))?;
        Self::handle_json(resp).await
    }

    /// For `204 No Content` responses.
    pub async fn delete_no_body(&self, path: &str) -> Result<(), CtlError> {
        let resp = self
            .http
            .delete(self.url(path))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| self.conn_error(e))?;
        Self::handle_status(resp).await
    }

    /// The WS auth contract is the query-param token, same as a browser `WebSocket`.
    pub fn ws_url(&self, path: &str) -> String {
        format!("ws://127.0.0.1:{}{path}?token={}", self.port, self.token)
    }

    async fn handle_status(resp: reqwest::Response) -> Result<(), CtlError> {
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(CtlError::Server(server_error_message(resp).await))
        }
    }

    async fn handle_json<T: DeserializeOwned>(resp: reqwest::Response) -> Result<T, CtlError> {
        if resp.status().is_success() {
            resp.json::<T>().await.map_err(|e| {
                CtlError::local(
                    format!("bad response from server: {e}"),
                    "check `yogurt ctl status`",
                )
            })
        } else {
            Err(server_error(resp).await)
        }
    }
}

async fn server_error(resp: reqwest::Response) -> CtlError {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let message = error_message_from(status, &body);
    let code = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|j| j.get("code")?.as_str().map(str::to_string));
    match code {
        Some(code) => CtlError::Coded { code, message },
        None => CtlError::Server(message),
    }
}

async fn server_error_message(resp: reqwest::Response) -> String {
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    error_message_from(status, &body)
}

/// The server answers some failures as `{"error": ...}` and others as plain text (axum rejections).
/// Only an empty body falls back to the bare status.
fn error_message_from(status: reqwest::StatusCode, body: &str) -> String {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(msg) = json.get("error").and_then(|v| v.as_str()) {
            return msg.to_string();
        }
    }
    let text = body.trim();
    if text.is_empty() || text.starts_with('{') || text.starts_with('<') {
        format!("server returned {status}")
    } else {
        format!("server returned {status}: {text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(port: u16) -> Instance {
        Instance {
            port,
            version: "0.0.0".to_string(),
            mode: "release".to_string(),
        }
    }

    #[test]
    fn error_message_keeps_json_and_plain_text_bodies() {
        let bad = reqwest::StatusCode::BAD_REQUEST;
        assert_eq!(
            error_message_from(bad, r#"{"error":"meeting not found"}"#),
            "meeting not found"
        );
        assert_eq!(
            error_message_from(bad, "unknown template \"nope\"; expected one of: general"),
            "server returned 400 Bad Request: unknown template \"nope\"; expected one of: general"
        );
        assert_eq!(
            error_message_from(bad, ""),
            "server returned 400 Bad Request"
        );
        assert_eq!(
            error_message_from(bad, r#"{"detail":"x"}"#),
            "server returned 400 Bad Request"
        );
    }

    #[test]
    fn flag_beats_env() {
        assert_eq!(port_precedence(Some(1), Some(2)), Some(1));
    }

    #[test]
    fn env_used_when_no_flag() {
        assert_eq!(port_precedence(None, Some(2)), Some(2));
    }

    #[test]
    fn neither_set_falls_through_to_scan() {
        assert_eq!(port_precedence(None, None), None);
    }

    #[test]
    fn scan_with_no_instances_is_no_server() {
        assert!(matches!(
            resolve_from_scan(vec![]),
            Err(CtlError::NoServer(None))
        ));
    }

    #[test]
    fn scan_with_one_instance_resolves_to_it() {
        let resolved = resolve_from_scan(vec![inst(7878)]).expect("single instance resolves");
        assert_eq!(resolved.port, 7878);
    }

    #[test]
    fn scan_with_two_instances_is_ambiguous() {
        match resolve_from_scan(vec![inst(7878), inst(7879)]) {
            Err(CtlError::Ambiguous(ports)) => assert_eq!(ports, vec![7878, 7879]),
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }
}
