//! `yogurt ctl settings` (CLI-6 / D1 second slice) -- read and write the
//! general settings the server exposes at `/api/settings`. Never touches a
//! provider or STT key -- that's `provider.rs`, and there is no `ctl`
//! subcommand that ever sets or reveals one (see
//! `docs/.planning/agent-workflow.md` section 5's "do not build" list).

use clap::Subcommand;
use serde_json::{json, Map, Value};

use super::client::{Client, CtlError};

#[derive(Subcommand, Debug)]
pub enum SettingsCmd {
    /// Print the current general settings.
    Get,
    /// Set one or more settings as name=value pairs, e.g. `stt_provider=cloud`.
    Set {
        #[arg(required = true)]
        pairs: Vec<String>,
        /// Print what would be sent, without sending it.
        #[arg(long)]
        dry_run: bool,
    },
}

pub async fn run(cmd: SettingsCmd, port: Option<u16>, json_out: bool) -> Result<(), CtlError> {
    match cmd {
        SettingsCmd::Get => get(port, json_out).await,
        SettingsCmd::Set { pairs, dry_run } => set(port, json_out, pairs, dry_run).await,
    }
}

/// Just the `general` half of `GET /api/settings` -- `providers` and
/// `presets` are `provider.rs`'s concern.
#[derive(Debug, serde::Deserialize)]
struct SettingsGetView {
    general: yogurt_db::settings::General,
}

fn print_general(json_out: bool, g: &yogurt_db::settings::General) {
    if json_out {
        println!("{}", json!(g));
    } else {
        println!("port: {}", g.port);
        println!("open_browser_on_start: {}", g.open_browser_on_start);
        println!("audio_input_device: {}", g.audio_input_device);
        println!("first_run_completed: {}", g.first_run_completed);
        println!("stt_provider: {}", g.stt_provider);
        println!("stt_model: {}", g.stt_model);
        println!("meeting_detection: {}", g.meeting_detection);
    }
}

async fn get(port: Option<u16>, json_out: bool) -> Result<(), CtlError> {
    let c = Client::discover(port).await?;
    let sv: SettingsGetView = c.get("/api/settings").await?;
    print_general(json_out, &sv.general);
    Ok(())
}

/// `key=value` -> a JSON field. `value` coerces to a bool/number when it
/// parses as one (`open_browser_on_start=true`, `port=7878`); anything
/// else, including a bare word like `cloud`, is sent as a plain JSON
/// string. The server owns the actual schema (`GeneralPatch`) and rejects
/// what it doesn't like -- see `validate_stt_patch` in
/// `crates/yogurt-server/src/api/settings.rs` -- so this never duplicates
/// that validation locally.
fn parse_pair(s: &str) -> Result<(String, Value), CtlError> {
    let (k, v) = s
        .split_once('=')
        .filter(|(k, _)| !k.is_empty())
        .ok_or_else(|| {
            CtlError::local(
                format!("invalid key=value pair: '{s}'"),
                "example: yogurt ctl settings set stt_provider=cloud",
            )
        })?;
    let value = serde_json::from_str::<Value>(v).unwrap_or_else(|_| Value::String(v.to_string()));
    Ok((k.to_string(), value))
}

async fn set(
    port_flag: Option<u16>,
    json_out: bool,
    pairs: Vec<String>,
    dry_run: bool,
) -> Result<(), CtlError> {
    let mut body = Map::new();
    for pair in &pairs {
        let (k, v) = parse_pair(pair)?;
        body.insert(k, v);
    }
    let body = Value::Object(body);

    if dry_run {
        if json_out {
            println!("{}", json!({ "dry_run": true, "would_send": body }));
        } else {
            println!("would PATCH /api/settings with: {body}");
        }
        return Ok(());
    }

    let c = Client::discover(port_flag).await?;
    let g: yogurt_db::settings::General = c.patch("/api/settings", &body).await?;
    print_general(json_out, &g);
    Ok(())
}
