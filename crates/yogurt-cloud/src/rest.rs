//! Client for one Supabase project's Auth and PostgREST endpoints.

use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::error::Error as _;
use std::fmt;
use std::time::Duration;

/// Supabase caps a response at 1,000 rows, so list calls page by this size.
const PAGE: usize = 1000;

pub struct Project {
    url: String,
    publishable_key: String,
    http: reqwest::Client,
}

#[derive(Clone)]
pub struct Session {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: String,
    /// Unix seconds.
    pub expires_at: i64,
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("user_id", &self.user_id)
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
    user: TokenUser,
}

#[derive(Deserialize)]
struct TokenUser {
    id: String,
}

impl From<TokenResponse> for Session {
    fn from(t: TokenResponse) -> Self {
        Session {
            access_token: t.access_token,
            refresh_token: t.refresh_token,
            user_id: t.user.id,
            expires_at: t.expires_at,
        }
    }
}

impl Project {
    pub fn new(url: &str, publishable_key: &str) -> Self {
        Self {
            url: url.trim_end_matches('/').to_string(),
            publishable_key: publishable_key.to_string(),
            http: reqwest::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(60))
                // A 307 or 308 would re-send the password to wherever it points.
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("static reqwest client config"),
        }
    }

    pub async fn sign_in(&self, email: &str, password: &str) -> Result<Session> {
        let req = self
            .token_request("password")
            .json(&json!({ "email": email, "password": password }));
        match send(req).await {
            Ok(r) => session_from(r).await,
            Err(Error::Http {
                status: 400, code, ..
            }) if code == "invalid_credentials" => Err(Error::BadCredentials),
            Err(e) => Err(e),
        }
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<Session> {
        let req = self
            .token_request("refresh_token")
            .json(&json!({ "refresh_token": refresh_token }));
        match send(req).await {
            Ok(r) => session_from(r).await,
            Err(Error::Http {
                status: 400 | 401, ..
            }) => Err(Error::SessionExpired),
            Err(e) => Err(e),
        }
    }

    /// Revokes this session's refresh token. Best effort: the caller ignores a failure.
    pub async fn logout(&self, session: &Session) -> Result<()> {
        let url = format!("{}/auth/v1/logout?scope=local", self.url);
        send(self.authed(self.http.post(url), session))
            .await
            .map(drop)
    }

    /// Sends with `return=minimal`: `authenticated` has no `select`, so asking for the rows back would fail.
    pub async fn insert(&self, session: &Session, table: &str, rows: &Value) -> Result<()> {
        let url = format!("{}/rest/v1/{table}", self.url);
        let req = self
            .authed(self.http.post(url), session)
            .header("Prefer", "return=minimal")
            .json(rows);
        send_authed(req).await.map(drop)
    }

    /// Calls a function that returns one value. A function returning null gives `Value::Null`.
    pub async fn rpc(&self, session: &Session, function: &str, args: &Value) -> Result<Value> {
        let url = format!("{}/rest/v1/rpc/{function}", self.url);
        let req = self.authed(self.http.post(url), session).json(args);
        send_authed(req).await?.json().await.map_err(network)
    }

    /// `query` is extra PostgREST parameters such as `select=meeting_id`. The function must order its rows or the pages are unstable.
    pub async fn rpc_all(
        &self,
        session: &Session,
        function: &str,
        args: &Value,
        query: &str,
    ) -> Result<Vec<Value>> {
        let mut all = Vec::new();
        loop {
            let url = format!(
                "{}/rest/v1/rpc/{function}?{query}&limit={PAGE}&offset={}",
                self.url,
                all.len()
            );
            let req = self.authed(self.http.post(url), session).json(args);
            let rows = json_rows(send_authed(req).await?).await?;
            if rows.is_empty() {
                return Ok(all);
            }
            all.extend(rows);
        }
    }

    fn token_request(&self, grant: &str) -> reqwest::RequestBuilder {
        self.http
            .post(format!("{}/auth/v1/token?grant_type={grant}", self.url))
            .header("apikey", &self.publishable_key)
    }

    fn authed(&self, rb: reqwest::RequestBuilder, session: &Session) -> reqwest::RequestBuilder {
        rb.header("apikey", &self.publishable_key)
            .bearer_auth(&session.access_token)
    }
}

async fn session_from(resp: reqwest::Response) -> Result<Session> {
    let t: TokenResponse = resp.json().await.map_err(network)?;
    Ok(t.into())
}

async fn json_rows(resp: reqwest::Response) -> Result<Vec<Value>> {
    resp.json().await.map_err(network)
}

fn network(e: reqwest::Error) -> Error {
    let e = e.without_url();
    let mut msg = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        msg.push_str(": ");
        msg.push_str(&s.to_string());
        source = s.source();
    }
    Error::Network(msg)
}

/// A 401 on a data call means the access token is dead, which the caller treats like an expired session.
async fn send_authed(req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
    match send(req).await {
        Err(Error::Http { status: 401, .. }) => Err(Error::SessionExpired),
        other => other,
    }
}

/// The error keeps the status and the server's error code and message, never the request body.
async fn send(req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
    let resp = req.send().await.map_err(network)?;
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    // A gateway error page is not JSON, and then the status line is all there is to say.
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    let text = |k: &str| match &body[k] {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    };
    Err(Error::Http {
        status: status.as_u16(),
        code: text("error_code")
            .or_else(|| text("code"))
            .unwrap_or_default(),
        message: text("msg")
            .or_else(|| text("message"))
            .or_else(|| text("error_description"))
            .unwrap_or_else(|| format!("Supabase returned {}", status.as_u16())),
    })
}
