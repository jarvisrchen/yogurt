//! `yogurt doctor` — diagnostic dump + repair actions.
//!
//! Ships four things (DIST-08 / ROADMAP Phase 9 Success Criterion 4):
//! 1. A default diagnostic dump (rust, macOS, Screen Recording permission,
//!    db path, configured providers, active STT, downloaded whisper models).
//! 2. `--json` for machine-readable bug reports.
//! 3. `--reset-screen-recording` — `tccutil reset ScreenCapture ai.yogurt.app`.
//! 4. `--check-port` and `--redownload-model <name>` repair actions.
//!
//! Reads the SAME `~/.yogurt/db.sqlite` settings store and the SAME
//! `yogurt_audio::permission` TCC check the running app uses — no separate
//! `config.toml` scrape, so this never drifts from what `yogurt start`
//! actually sees. NEVER prints API key values, only provider names
//! (D-12 / providers::list_names already excludes secrets by construction).

use anyhow::{Context, Result};
use serde::Serialize;
use std::io::ErrorKind;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;

/// Bundle ID pinned across the notarization config (plan 09-01) and this
/// TCC reset — they must match for the reset to actually clear TCC state.
const BUNDLE_ID: &str = "ai.yogurt.app";

/// Port `yogurt start` binds to by default; used by `--check-port`.
const DEFAULT_PORT: u16 = 7878;

pub struct DoctorArgs {
    pub json: bool,
    pub reset_screen_recording: bool,
    pub check_port: bool,
    pub redownload_model: Option<String>,
}

#[derive(Debug, Serialize)]
struct Report {
    service: &'static str,
    version: &'static str,
    rust: &'static str,
    macos: String,
    screen_recording: String,
    db_path: String,
    db_exists: bool,
    providers: Vec<String>,
    stt: String,
    models: Vec<String>,
}

pub async fn run(args: DoctorArgs) -> Result<()> {
    if args.reset_screen_recording {
        return reset_screen_recording();
    }
    if args.check_port {
        return check_port(DEFAULT_PORT);
    }
    if let Some(model) = args.redownload_model {
        return redownload_model(&model);
    }

    let report = build_report();
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_human(&report);
    }
    Ok(())
}

fn reset_screen_recording() -> Result<()> {
    let status = Command::new("tccutil")
        .args(["reset", "ScreenCapture", BUNDLE_ID])
        .status()
        .context("failed to run tccutil (this repair action is macOS-only)")?;
    if status.success() {
        println!(
            "Screen Recording permission reset for {BUNDLE_ID}. \
             Restart yogurt and grant access again when prompted."
        );
    } else {
        println!("tccutil exited with {status}; permission may not have been reset.");
    }
    Ok(())
}

fn check_port(port: u16) -> Result<()> {
    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(_) => println!("port {port} is free"),
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            let suggestion = port.saturating_add(1);
            println!("port {port} is in use -- try `yogurt start --port {suggestion}`");
        }
        Err(e) => return Err(e).context("checking port"),
    }
    Ok(())
}

fn redownload_model(name: &str) -> Result<()> {
    let Some(home) = yogurt_home() else {
        anyhow::bail!("could not resolve home directory");
    };
    let path = home.join("models").join(format!("ggml-{name}.bin"));
    println!("removing {} (will re-download on next use)", path.display());
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {
            println!("no local file for model '{name}' -- nothing to remove");
        }
        Err(e) => return Err(e).context("removing model file"),
    }
    Ok(())
}

fn yogurt_home() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| b.home_dir().join(".yogurt"))
}

/// Collect the diagnostic report. Never fails outright -- every field
/// degrades to a "couldn't determine" string/empty value on error, so a
/// broken environment is exactly the case this command needs to survive.
fn build_report() -> Report {
    let home = yogurt_home();
    let db_path = home.as_ref().map(|h| h.join("db.sqlite"));
    let db_exists = db_path.as_ref().is_some_and(|p| p.exists());

    // Only open (and thus run migrations against) the DB if it already
    // exists -- a diagnostic command must not create `~/.yogurt/db.sqlite`
    // as a side effect on a machine that has never run `yogurt start`.
    let (providers, stt) = if db_exists {
        match yogurt_db::Db::open_default() {
            Ok(db) => {
                let providers = yogurt_db::providers::list_names(&db).unwrap_or_default();
                let stt = yogurt_db::settings::load_general(&db)
                    .map(|g| g.stt_provider)
                    .unwrap_or_else(|_| "unknown".to_string());
                (providers, stt)
            }
            Err(_) => (Vec::new(), "unknown (failed to open db)".to_string()),
        }
    } else {
        (
            Vec::new(),
            "not configured yet -- run `yogurt start` first".to_string(),
        )
    };

    let models = home
        .as_ref()
        .map(|h| h.join("models"))
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .map(|entries| {
            let mut names: Vec<String> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|name| name.starts_with("ggml-") && name.ends_with(".bin"))
                .collect();
            names.sort();
            names
        })
        .unwrap_or_default();

    Report {
        service: "yogurt-doctor",
        version: env!("CARGO_PKG_VERSION"),
        rust: env!("YOGURT_RUSTC_VERSION"),
        macos: macos_version(),
        screen_recording: screen_recording_status(),
        db_path: db_path
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        db_exists,
        providers,
        stt,
        models,
    }
}

fn macos_version() -> String {
    Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn screen_recording_status() -> String {
    use yogurt_audio::permission::PermissionStatus;
    match yogurt_audio::permission::has_screen_recording_permission() {
        PermissionStatus::Granted => "granted".to_string(),
        PermissionStatus::Denied => {
            "denied (open System Settings > Privacy > Screen Recording)".to_string()
        }
        PermissionStatus::NotDetermined => "not determined (not yet prompted)".to_string(),
        PermissionStatus::NotRequired => "not required (non-macOS)".to_string(),
    }
}

fn print_human(r: &Report) {
    println!("yogurt doctor");
    println!("version: {}", r.version);
    println!("rust: {}", r.rust);
    println!("macos: {}", r.macos);
    println!("screen recording: {}", r.screen_recording);
    println!("db path: {}", r.db_path);
    println!("db exists: {}", r.db_exists);
    if r.providers.is_empty() {
        println!("providers: none configured");
    } else {
        println!("providers: {}", r.providers.join(", "));
    }
    println!("stt: {}", r.stt);
    if r.models.is_empty() {
        println!("models: none downloaded");
    } else {
        println!("models: {}", r.models.join(", "));
    }
    println!("config: {}", r.db_path);
    println!("notes: use --json for a machine-readable dump; --reset-screen-recording,");
    println!("       --check-port, and --redownload-model <name> are repair actions.");
    println!();
    println!("paste this output into any issue at https://github.com/jarvisrchen/yogurt/issues");
}
