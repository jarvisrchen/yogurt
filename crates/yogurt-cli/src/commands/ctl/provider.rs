//! Never prints a key value, only `key: set` / `key: missing`.
//! There is no `ctl provider key set` on purpose: keys would land in shell history and transcripts.

use clap::Subcommand;
use serde::Deserialize;
use serde_json::json;

use super::client::{Client, CtlError};

#[derive(Subcommand, Debug)]
pub enum ProviderCmd {
    /// List configured providers and which one is active.
    List,
    /// Switch the active provider.
    Activate { name: String },
    /// Test a provider's connection with one real completion. Defaults to
    /// the active provider.
    Test { name: Option<String> },
}

pub async fn run(cmd: ProviderCmd, port: Option<u16>, json_out: bool) -> Result<(), CtlError> {
    match cmd {
        ProviderCmd::List => list(port, json_out).await,
        ProviderCmd::Activate { name } => activate(port, json_out, &name).await,
        ProviderCmd::Test { name } => test(port, json_out, name).await,
    }
}

/// Wire shape of `GET /api/settings/providers`.
/// Never carries `api_key_masked` past [`key_state`]; even the masked form is not printed.
#[derive(Debug, Deserialize)]
struct ProviderRow {
    id: String,
    name: String,
    model: String,
    is_active: bool,
    api_key_masked: Option<String>,
    adapter: String,
}

fn key_state(row: &ProviderRow) -> &'static str {
    if row.api_key_masked.is_some() {
        "set"
    } else {
        "missing"
    }
}

async fn list(port: Option<u16>, json_out: bool) -> Result<(), CtlError> {
    let c = Client::discover(port).await?;
    let rows: Vec<ProviderRow> = c.get("/api/settings/providers").await?;
    if json_out {
        let providers: Vec<_> = rows
            .iter()
            .map(|r| {
                json!({
                    "name": r.name,
                    "model": r.model,
                    "adapter": r.adapter,
                    "is_active": r.is_active,
                    "key": key_state(r),
                })
            })
            .collect();
        println!("{}", json!({ "providers": providers }));
    } else {
        println!("{} provider(s)", rows.len());
        for r in &rows {
            let active = if r.is_active { "active" } else { "-" };
            println!(
                "{active:<6} {:<20} {:<8} {:<20} key: {}",
                r.name,
                r.adapter,
                r.model,
                key_state(r)
            );
        }
    }
    Ok(())
}

fn find<'a>(rows: &'a [ProviderRow], name: &str) -> Result<&'a ProviderRow, CtlError> {
    rows.iter().find(|r| r.name == name).ok_or_else(|| {
        CtlError::local(
            format!("no provider named '{name}'"),
            "run `yogurt ctl provider list`",
        )
    })
}

async fn activate(port: Option<u16>, json_out: bool, name: &str) -> Result<(), CtlError> {
    let c = Client::discover(port).await?;
    let rows: Vec<ProviderRow> = c.get("/api/settings/providers").await?;
    let row = find(&rows, name)?;
    let _: ProviderRow = c
        .post_empty(&format!("/api/settings/providers/{}/activate", row.id))
        .await?;
    if json_out {
        println!("{}", json!({ "status": "activated", "name": name }));
    } else {
        println!("activated {name}");
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct TestResult {
    ok: bool,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

async fn test(port: Option<u16>, json_out: bool, name: Option<String>) -> Result<(), CtlError> {
    let c = Client::discover(port).await?;
    let rows: Vec<ProviderRow> = c.get("/api/settings/providers").await?;
    let row = match &name {
        Some(n) => find(&rows, n)?,
        None => rows.iter().find(|r| r.is_active).ok_or_else(|| {
            CtlError::local(
                "no active provider",
                "run `yogurt ctl provider activate <name>`",
            )
        })?,
    };
    let result: TestResult = c
        .post(
            &format!("/api/settings/providers/{}/test", row.id),
            &json!({}),
        )
        .await?;
    if json_out {
        println!(
            "{}",
            json!({ "name": row.name, "ok": result.ok, "model": result.model, "error": result.error })
        );
    } else if result.ok {
        println!(
            "ok: {} ({})",
            row.name,
            result.model.as_deref().unwrap_or("?")
        );
    } else {
        // Not a `CtlError`: the server treats a rejected key as a successful probe, so this is exit 0 informational output.
        println!(
            "failed: {}: {}",
            row.name,
            result.error.as_deref().unwrap_or("unknown error")
        );
    }
    Ok(())
}
