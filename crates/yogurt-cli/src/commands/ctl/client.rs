//! HTTP client + server discovery for `yogurt ctl` (CLI-4 / D1).
//!
//! One `reqwest::Client`, one auth story (the session token file), one
//! discovery precedence: `--port` flag, then `$YOGURT_PORT`, then a health
//! scan of the port range `just dev` / `port-guard.sh` already use
//! (7878-7898). Every failure mode collapses to a `CtlError`, which the
//! caller turns into the `error: ... / help: ...` pair on stdout.

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

/// `just dev` / `scripts/lib/port-guard.sh` widen a busy default port by up
/// to 20 before giving up, so this is the same window a scan has to cover
/// to find an instance a second worktree pushed off 7878.
pub const PORT_RANGE: std::ops::RangeInclusive<u16> = 7878..=7898;

/// A single probe result: `GET /api/health` answered, and (once D5 has
/// landed on the running binary) carried `version` + `mode`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Instance {
    pub port: u16,
    pub version: String,
    pub mode: String,
}

/// Every business-logic failure `ctl` can hit, already carrying its own
/// `help:` line. Never a raw `anyhow`/`reqwest` error at the print site --
/// see `mod.rs::print_error`.
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
    /// Anything else: a bad local path, a malformed response, a connection
    /// that dropped mid-request. Carries its own tailored help text.
    Local { message: String, help: String },
}

impl CtlError {
    pub fn local(message: impl Into<String>, help: impl Into<String>) -> Self {
        CtlError::Local {
            message: message.into(),
            help: help.into(),
        }
    }

    /// The `(error, help)` pair `mod.rs` prints as `error: ...` / `help: ...`.
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
            CtlError::Server(message) => (message.clone(), "check `yogurt ctl status`".to_string()),
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

/// Probe every port in [`PORT_RANGE`] concurrently. `tokio::task::JoinSet`
/// rather than `futures::future::join_all` -- tokio is already a
/// dependency via `yogurt-server`, so this adds no new crate.
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

/// What `ctl status` shows -- unlike [`resolve`], an ambiguous scan is not
/// an error here, it's the answer.
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

/// Discovery for every command except `status`: resolves to exactly one
/// instance or fails with a message telling the caller what to do.
pub async fn resolve(http: &reqwest::Client, explicit: Option<u16>) -> Result<Instance, CtlError> {
    if let Some(port) = port_precedence(explicit, env_port()) {
        return probe_health(http, port)
            .await
            .ok_or(CtlError::NoServer(Some(port)));
    }
    resolve_from_scan(scan(http).await)
}

/// Pure: `--port` beats `$YOGURT_PORT`. Takes both already-read values
/// rather than reading `$YOGURT_PORT` itself, so it's unit-testable
/// without touching the process env.
fn port_precedence(flag: Option<u16>, env: Option<u16>) -> Option<u16> {
    flag.or(env)
}

/// Pure: given however many instances a scan found, decide the outcome.
/// Takes the already-run scan result rather than probing itself, so
/// it's unit-testable with a synthetic `Vec<Instance>` and no network.
fn resolve_from_scan(found: Vec<Instance>) -> Result<Instance, CtlError> {
    match found.len() {
        0 => Err(CtlError::NoServer(None)),
        1 => Ok(found.into_iter().next().expect("len == 1")),
        _ => Err(CtlError::Ambiguous(
            found.into_iter().map(|i| i.port).collect(),
        )),
    }
}

pub(crate) fn yogurt_home() -> Result<PathBuf, CtlError> {
    directories::BaseDirs::new()
        .map(|b| b.home_dir().join(".yogurt"))
        .ok_or_else(|| CtlError::local("could not resolve home directory", "check $HOME is set"))
}

/// Read the local session token. Every authenticated route needs it; no
/// `ctl` subcommand ever prints its value (see `tests/ctl_smoke.rs`'s
/// `--help`/`status` assertions).
fn read_session_token() -> Result<String, CtlError> {
    let path = yogurt_home()?.join("session-token");
    std::fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .map_err(|_| {
            CtlError::local(
                format!("could not read session token at {}", path.display()),
                "run `yogurt start` first",
            )
        })
}

/// The authenticated HTTP surface `ctl` talks to.
pub struct Client {
    http: reqwest::Client,
    pub port: u16,
    token: String,
}

impl Client {
    pub fn new(http: reqwest::Client, port: u16, token: String) -> Self {
        Self { http, port, token }
    }

    /// Resolve a target server (discovery precedence) and load the local
    /// session token. The one entry point every subcommand but `status`,
    /// `detect`, and `windows` uses.
    pub async fn discover(explicit_port: Option<u16>) -> Result<Self, CtlError> {
        let http = reqwest::Client::new();
        let instance = resolve(&http, explicit_port).await?;
        let token = read_session_token()?;
        Ok(Self::new(http, instance.port, token))
    }

    /// Build a client that talks to a specific port without going through
    /// discovery -- used when a meeting URL already named the port.
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

    /// Same as [`Self::post_empty`] but for endpoints that answer with no
    /// body at all (e.g. the 202-Accepted-and-fire-and-forget model
    /// download start) -- `post_empty`'s `resp.json::<T>()` would fail
    /// parsing an empty body.
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

    /// Same as [`Self::delete`] but for `204 No Content` responses (e.g.
    /// `DELETE /api/meetings/:id`).
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

    /// `ws://127.0.0.1:<port><path>?token=<token>` -- the WS auth contract
    /// (`crate::ws`'s doc comment on the server side) is the query-param
    /// token, same as every browser `WebSocket` connection.
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
            Err(CtlError::Server(server_error_message(resp).await))
        }
    }
}

async fn server_error_message(resp: reqwest::Response) -> String {
    let status = resp.status();
    match resp.json::<serde_json::Value>().await {
        Ok(body) => body
            .get("error")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("server returned {status}")),
        Err(_) => format!("server returned {status}"),
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
