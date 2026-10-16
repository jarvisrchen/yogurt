//! Tracing setup: stderr as before, plus a daily-rotated file under
//! `<data dir>/logs/` so a user can send logs from another machine.
//! The file never receives anything the stderr layer would not: yogurt's own
//! `tracing` events, which carry no keys, tokens, transcript text or notes.

use std::path::{Path, PathBuf};
use tracing_appender::{non_blocking::WorkerGuard, rolling};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

/// Daily files named `yogurt.log.YYYY-MM-DD`, newest 14 kept.
const LOG_PREFIX: &str = "yogurt.log";
const LOG_FILES_KEPT: usize = 14;

fn filter() -> EnvFilter {
    // EnvFilter matches targets by prefix, so `yogurt=info` already covers
    // `yogurt_server`, `yogurt_audio`, `yogurt_stt`, and every other `yogurt_*`
    // crate.
    //
    // `whisper_rs=error` covers the C-side logs that `install_logging_hooks()`
    // redirects into `tracing`: ggml and whisper.cpp emit their backend/model
    // banners at INFO on every decode, which is noise. CLI-2 dropped their WARN
    // level too, because the one that actually shows up is ggml's `tensor API
    // disabled for pre-M5 and pre-A19 devices`, a hardware-capability note on
    // every load of every model on this machine. A real decode failure still
    // surfaces as an ERROR here, and as an `stt_error` frame in the UI.
    // `RUST_LOG=whisper_rs=debug` brings the whole stream back.
    EnvFilter::try_from_default_env().unwrap_or_else(|_| "yogurt=info,whisper_rs=error".into())
}

/// `~/.yogurt` unless `--data-dir` / `$YOGURT_DATA_DIR` says otherwise.
fn log_dir(data_dir: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    let root = match crate::data_dir::resolve(data_dir)? {
        Some(dir) => dir,
        None => directories::BaseDirs::new()
            .map(|b| b.home_dir().join(".yogurt"))
            .ok_or_else(|| anyhow::anyhow!("no home directory"))?,
    };
    Ok(root.join("logs"))
}

fn file_writer(
    dir: &Path,
) -> anyhow::Result<(tracing_appender::non_blocking::NonBlocking, WorkerGuard)> {
    // The appender prunes old files before it creates the directory, which
    // prints an error to stderr on a first run.
    std::fs::create_dir_all(dir)?;
    let appender = rolling::Builder::new()
        .rotation(rolling::Rotation::DAILY)
        .filename_prefix(LOG_PREFIX)
        .max_log_files(LOG_FILES_KEPT)
        .build(dir)?;
    Ok(tracing_appender::non_blocking(appender))
}

/// Install the global subscriber. `with_file` is true only for `yogurt start`,
/// so short-lived commands such as `ctl` never create a log directory. A log
/// file that cannot be opened degrades to stderr only. Keep the returned guard
/// alive for the process lifetime, since dropping it flushes the file.
pub fn init(with_file: bool, data_dir: Option<PathBuf>) -> Option<WorkerGuard> {
    let stderr = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(filter());
    let registry = tracing_subscriber::registry().with(stderr);
    let file = if with_file {
        log_dir(data_dir)
            .and_then(|dir| file_writer(&dir).map(|w| (dir, w)))
            .map_err(|e| eprintln!("warning: file logging disabled: {e:#}"))
            .ok()
    } else {
        None
    };
    match file {
        Some((_dir, (writer, guard))) => {
            registry
                .with(
                    tracing_subscriber::fmt::layer()
                        .with_writer(writer)
                        .with_ansi(false)
                        .with_filter(filter()),
                )
                .init();
            Some(guard)
        }
        None => {
            registry.init();
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_dir_follows_the_data_dir_override() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = log_dir(Some(tmp.path().to_path_buf())).unwrap();
        assert_eq!(dir, tmp.path().join("logs"));
    }

    #[test]
    fn file_layer_writes_events_under_logs() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = log_dir(Some(tmp.path().to_path_buf())).unwrap();
        let (writer, guard) = file_writer(&dir).unwrap();
        let subscriber = tracing_subscriber::registry().with(
            tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_filter(EnvFilter::new("yogurt=info")),
        );
        tracing::subscriber::with_default(subscriber, || {
            tracing::info!(target: "yogurt_audio", "hello from the file layer");
            tracing::debug!(target: "yogurt_audio", "below the file level");
        });
        drop(guard);

        let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1);
        let name = entries[0].file_name().into_string().unwrap();
        assert!(name.starts_with("yogurt.log."), "unexpected name {name}");
        let text = std::fs::read_to_string(entries[0].path()).unwrap();
        assert!(text.contains("hello from the file layer"));
        assert!(!text.contains("below the file level"));
    }
}
