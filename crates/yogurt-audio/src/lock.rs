//! AUD-8: orphaned-capture-session detection.
//!
//! A force-killed `yogurt` process skips `AudioStream::Drop`, so its SCK +
//! cpal handles never get torn down. macOS then blocks the *next*
//! `start_capture()` for several minutes inside `SCStream::start_capture()`
//! while it reclaims the dead process's resources — a direct retry after
//! that window opens instantly. We can't shorten that OS-level reclaim, so
//! instead we detect the condition up front (a PID marker file that points
//! at a process which is no longer alive) and fail fast with an actionable
//! error instead of hanging silently.

use crate::error::{AudioError, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

fn lock_path() -> Result<PathBuf> {
    let base = directories::BaseDirs::new().ok_or_else(|| {
        AudioError::SystemCaptureFailed("could not resolve home directory".into())
    })?;
    let dir = base.home_dir().join(".yogurt");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("capture.lock"))
}

/// True if a process with this PID is alive. `kill(pid, 0)` sends no
/// signal — it's the standard liveness probe. `ESRCH` means gone; `EPERM`
/// still means alive (owned by someone else). Any other error is treated
/// conservatively as "alive" so we never clobber a live session's marker.
#[cfg(target_os = "macos")]
fn pid_is_alive(pid: i32) -> bool {
    // SAFETY: kill(pid, 0) is a pure liveness probe, no signal is sent.
    if unsafe { libc::kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

/// RAII guard: removes the marker file on drop, i.e. a clean shutdown
/// (`Registry::stop()`'s graceful teardown or normal `AudioStream::Drop`).
#[derive(Debug)]
pub struct CaptureLock(PathBuf);

impl Drop for CaptureLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Acquire the capture marker before opening SCK/cpal.
///
/// - No marker present: write ours, proceed.
/// - Marker with a live PID: another yogurt process is already recording —
///   fail immediately with [`AudioError::AlreadyRecording`].
/// - Marker with a dead PID: the AUD-8 orphaned-session case. Log a clear
///   warning, remove the stale marker (so it doesn't wedge every future
///   attempt), and fail fast with [`AudioError::OrphanedSession`] instead
///   of letting the caller hang inside SCK for minutes.
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
                "AUD-8: found an orphaned capture marker — a previous yogurt process was \
                 likely force-killed mid-recording; macOS may still be reclaiming its SCK/mic \
                 resources for several minutes"
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

    #[test]
    fn pid_is_alive_true_for_self() {
        assert!(pid_is_alive(std::process::id() as i32));
    }

    #[test]
    fn pid_is_alive_false_for_dead_pid() {
        // Spawn, wait, and reap a short-lived child so its PID is
        // guaranteed dead and not recycled onto anything else we own.
        let mut child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("true")
            .spawn()
            .expect("spawn /bin/sh");
        let pid = child.id() as i32;
        child.wait().expect("wait for child");
        assert!(!pid_is_alive(pid));
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
        // The live marker is left in place — it's not ours to clean up.
        assert!(path.exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn acquire_at_clears_stale_marker_and_fails_fast() {
        let dir = std::env::temp_dir().join(format!("yogurt-audio-test3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("capture.lock");

        let mut child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("true")
            .spawn()
            .expect("spawn /bin/sh");
        let dead_pid = child.id();
        child.wait().expect("wait for child");
        std::fs::write(&path, dead_pid.to_string()).unwrap();

        let err = acquire_at(&path).expect_err("should fail fast on an orphaned marker");
        assert!(matches!(err, AudioError::OrphanedSession));
        assert!(!path.exists(), "stale marker should be cleared");

        // The next attempt (marker now gone) proceeds normally.
        let guard = acquire_at(&path).expect("second attempt should succeed");
        drop(guard);

        std::fs::remove_dir_all(&dir).ok();
    }
}
