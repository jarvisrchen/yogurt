//! Orphaned-capture-session detection.
//!
//! A force-killed `yogurt` process skips `AudioStream::Drop`, so its SCK +
//! cpal handles never get torn down, and macOS blocks the *next*
//! `start_capture()` for several minutes reclaiming them. We can't shorten
//! that OS-level reclaim, so instead we mark our own capture session with
//! a PID file and fail fast when the marker points at a dead process,
//! rather than hanging silently inside SCK.
//!
//! ponytail: PID reuse is a false-positive ceiling here - if the OS recycles
//! the dead PID onto an unrelated live process before we check, we'd treat
//! a genuinely free capture session as "already recording" and refuse to
//! start. Rare in practice and the escape hatch is simple:
//! `rm <data_dir>/capture.lock`. Upgrade to a process-start-time check if
//! this ever bites.

use crate::error::{AudioError, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

/// `~/.yogurt`, overridable via `$YOGURT_DATA_DIR` (mirrors
/// `yogurt-cli::data_dir::resolve`'s env precedence) so a worktree instance
/// doesn't collide with the main one on the same lock file.
fn data_dir() -> Result<PathBuf> {
    let dir = match std::env::var_os("YOGURT_DATA_DIR") {
        Some(dir) => PathBuf::from(dir),
        None => directories::BaseDirs::new()
            .ok_or_else(|| {
                AudioError::SystemCaptureFailed("could not resolve home directory".into())
            })?
            .home_dir()
            .join(".yogurt"),
    };
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn lock_path() -> Result<PathBuf> {
    Ok(data_dir()?.join("capture.lock"))
}

#[cfg(target_os = "macos")]
fn pid_is_alive(pid: i32) -> bool {
    // SAFETY: kill(pid, 0) is a pure liveness probe, no signal is sent.
    let killed = unsafe { libc::kill(pid, 0) };
    killed == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// Removes the capture marker on drop (clean shutdown).
#[derive(Debug)]
pub struct CaptureLock(PathBuf);

impl Drop for CaptureLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[cfg(target_os = "macos")]
pub fn acquire() -> Result<CaptureLock> {
    acquire_at(&lock_path()?)
}

fn acquire_at(path: &Path) -> Result<CaptureLock> {
    if let Ok(contents) = std::fs::read_to_string(path) {
        if let Ok(pid) = contents.trim().parse::<i32>() {
            if pid_is_alive(pid) {
                return Err(AudioError::AlreadyRecording);
            }
            tracing::warn!(
                stale_pid = pid,
                "orphaned capture marker - a previous yogurt process was likely force-killed \
                 mid-recording; macOS may still be reclaiming its SCK/mic resources for \
                 several minutes"
            );
        }
        let _ = std::fs::remove_file(path);
        return Err(AudioError::OrphanedSession);
    }

    let mut f = std::fs::File::create(path)?;
    write!(f, "{}", std::process::id())?;
    Ok(CaptureLock(path.to_path_buf()))
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    fn dead_pid() -> u32 {
        let mut child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("true")
            .spawn()
            .expect("spawn /bin/sh");
        let pid = child.id();
        child.wait().expect("wait for child");
        pid
    }

    #[test]
    fn pid_is_alive_true_for_self() {
        assert!(pid_is_alive(std::process::id() as i32));
    }

    #[test]
    fn pid_is_alive_false_for_dead_pid() {
        assert!(!pid_is_alive(dead_pid() as i32));
    }

    #[test]
    fn acquire_at_writes_marker_and_cleans_up_on_drop() {
        let dir = std::env::temp_dir().join(format!("yogurt-audio-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("capture.lock");

        let guard = acquire_at(&path).expect("first acquire should succeed");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().trim(),
            std::process::id().to_string()
        );
        drop(guard);
        assert!(!path.exists(), "marker should be removed on drop");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn acquire_at_rejects_when_marker_pid_is_alive() {
        let dir = std::env::temp_dir().join(format!("yogurt-audio-test2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("capture.lock");
        std::fs::write(&path, std::process::id().to_string()).unwrap();

        let err = acquire_at(&path).expect_err("should reject a live-owner marker");
        assert!(matches!(err, AudioError::AlreadyRecording));
        assert!(path.exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn acquire_at_clears_stale_marker_and_fails_fast() {
        let dir = std::env::temp_dir().join(format!("yogurt-audio-test3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("capture.lock");
        std::fs::write(&path, dead_pid().to_string()).unwrap();

        let err = acquire_at(&path).expect_err("should fail fast on an orphaned marker");
        assert!(matches!(err, AudioError::OrphanedSession));
        assert!(!path.exists(), "stale marker should be cleared");

        let guard = acquire_at(&path).expect("second attempt should succeed");
        drop(guard);

        std::fs::remove_dir_all(&dir).ok();
    }
}
