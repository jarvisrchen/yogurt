//! The one Yogurt Cloud push that may run at a time, and what a reconnecting client reads about it.

use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use yogurt_cloud::push::{Pacing, Progress};

struct Running {
    cancel: watch::Sender<bool>,
    sent: usize,
    total: usize,
    /// Unix milliseconds the current retry wait ends.
    retry_until: Option<i64>,
    /// Whether a client was told about this pass, which only happens when it needs more than one batch.
    announced: bool,
    /// When the plan was made and what had been sent then, which the pace is measured from.
    started: Option<(i64, usize)>,
    /// When the last batch landed and the milliseconds left at the pace so far then.
    eta: Option<(i64, i64)>,
}

impl Running {
    /// Counts down from the last landing, so the time left does not climb between batches.
    fn left_secs(&self, now: i64) -> Option<u64> {
        let (at, ms) = self.eta?;
        Some(((ms - (now - at)).max(0) / 1000) as u64)
    }
}

/// What a push would send now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    pub meetings: usize,
    pub not_pushed: usize,
    /// Bytes of the changed meetings' bodies.
    pub bytes: usize,
    pub estimate_secs: u64,
}

/// The last "N not pushed" answer and what the library looked like when it was worked out.
struct Counted {
    fingerprint: String,
    at: Instant,
    pending: Pending,
}

/// A backstop for a change the fingerprint cannot see.
const COUNT_TTL: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct PushJob {
    pacing: Pacing,
    inner: Mutex<Option<Running>>,
    counted: Mutex<Option<Counted>>,
    /// When a meeting last stopped or finished its notes, which the `meeting` auto-push setting waits on.
    requested: Mutex<Option<i64>>,
}

impl PushJob {
    pub fn new(pacing: Pacing) -> Self {
        Self {
            pacing,
            ..Self::default()
        }
    }

    /// The remembered count, if the library still looks as it did when it was counted.
    pub fn counted(&self, fingerprint: &str) -> Option<Pending> {
        let g = self.counted.lock().unwrap();
        let c = g.as_ref()?;
        (c.fingerprint == fingerprint && c.at.elapsed() < COUNT_TTL).then_some(c.pending)
    }

    pub fn remember(&self, fingerprint: String, pending: Pending) {
        *self.counted.lock().unwrap() = Some(Counted {
            fingerprint,
            at: Instant::now(),
            pending,
        });
    }

    pub fn request(&self, now: i64) {
        *self.requested.lock().unwrap() = Some(now);
    }

    pub fn requested(&self) -> Option<i64> {
        *self.requested.lock().unwrap()
    }

    pub fn pacing(&self) -> &Pacing {
        &self.pacing
    }

    /// `None` when a push is already running.
    pub fn begin(&self) -> Option<watch::Receiver<bool>> {
        let mut g = self.inner.lock().unwrap();
        if g.is_some() {
            return None;
        }
        let (cancel, rx) = watch::channel(false);
        *g = Some(Running {
            cancel,
            sent: 0,
            total: 0,
            retry_until: None,
            announced: false,
            started: None,
            eta: None,
        });
        Some(rx)
    }

    pub fn is_running(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }

    /// Asks a running push to stop at its next batch boundary or retry wait.
    pub fn cancel(&self) {
        if let Some(r) = self.inner.lock().unwrap().as_ref() {
            let _ = r.cancel.send(true);
        }
    }

    /// Records a pass's progress. It returns whether clients should hear of it, which is true for a pass of more than one batch.
    pub fn update(&self, p: &Progress, now: i64) -> bool {
        let mut g = self.inner.lock().unwrap();
        let Some(r) = g.as_mut() else {
            return false;
        };
        let landed = p.sent > r.sent;
        r.sent = p.sent;
        r.total = p.total;
        r.retry_until = p.retry_in.map(|d| now + d.as_millis() as i64);
        r.announced |= p.batches > 1;
        let (at, from) = *r.started.get_or_insert((now, p.sent));
        if landed && p.sent > from {
            let left = p.total.saturating_sub(p.sent) as i64;
            r.eta = Some((now, (now - at) * left / (p.sent - from) as i64));
        }
        r.announced
    }

    pub fn left_secs(&self, now: i64) -> Option<u64> {
        self.inner.lock().unwrap().as_ref()?.left_secs(now)
    }

    /// Ends the push and says whether clients were told about it. A second call is a no-op.
    /// A push rewrites hashes without changing their count or length, which the fingerprint cannot see, so it also drops the remembered count.
    pub fn finish(&self) -> bool {
        *self.counted.lock().unwrap() = None;
        self.inner
            .lock()
            .unwrap()
            .take()
            .is_some_and(|r| r.announced)
    }

    /// `null` when idle. Otherwise the counts so far, the seconds left before a retry, and the seconds left in the pass.
    pub fn status(&self, now: i64) -> Value {
        match self.inner.lock().unwrap().as_ref() {
            None => Value::Null,
            Some(r) => json!({
                "sent": r.sent,
                "total": r.total,
                "retry_in": r.retry_until.map(|t| ((t - now).max(0) + 999) / 1000),
                "announced": r.announced,
                "left_secs": r.left_secs(now),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn progress(sent: usize, batches: usize, retry_in: Option<u64>) -> Progress {
        Progress {
            sent,
            total: 100,
            batches,
            retry_in: retry_in.map(Duration::from_secs),
        }
    }

    #[test]
    fn one_push_at_a_time_and_a_single_batch_stays_quiet() {
        let job = PushJob::default();
        assert!(job.status(0).is_null());
        let _rx = job.begin().expect("first start");
        assert!(job.begin().is_none(), "second start is refused");
        assert!(!job.update(&progress(0, 1, None), 0));
        assert!(!job.finish(), "a one-batch pass was never announced");
        assert!(job.begin().is_some(), "free again after finish");
    }

    #[test]
    fn status_counts_down_a_retry_wait() {
        let job = PushJob::default();
        let _rx = job.begin().unwrap();
        assert!(job.update(&progress(40, 4, Some(45)), 1_000));
        let s = job.status(1_000 + 10_500);
        assert_eq!(
            (s["sent"].as_u64(), s["total"].as_u64()),
            (Some(40), Some(100))
        );
        assert_eq!(s["retry_in"], 35);
        assert!(job.update(&progress(50, 4, None), 20_000));
        assert!(job.status(20_000)["retry_in"].is_null());
        assert!(job.finish());
        assert!(!job.finish());
    }

    #[test]
    fn a_count_is_reused_only_while_the_library_looks_the_same() {
        let job = PushJob::default();
        assert_eq!(job.counted("a"), None);
        let p = Pending {
            meetings: 1400,
            not_pushed: 12,
            ..Pending::default()
        };
        job.remember("a".into(), p);
        assert_eq!(job.counted("a"), Some(p));
        assert_eq!(job.counted("b"), None);
    }

    #[test]
    fn time_left_follows_the_pace_since_the_plan() {
        let job = PushJob::default();
        let _rx = job.begin().unwrap();
        job.update(&progress(0, 4, None), 1_000);
        assert_eq!(job.left_secs(1_000), None, "nothing sent yet");
        job.update(&progress(25, 4, None), 11_000);
        assert_eq!(job.left_secs(11_000), Some(30));
        assert_eq!(
            job.status(13_000)["left_secs"],
            28,
            "counts down between batches"
        );
        job.update(&progress(25, 4, Some(45)), 14_000);
        assert_eq!(
            job.left_secs(14_000),
            Some(27),
            "a retry report is not a landing"
        );
    }

    #[test]
    fn cancel_reaches_the_running_pass() {
        let job = PushJob::default();
        job.cancel();
        let rx = job.begin().unwrap();
        assert!(!*rx.borrow());
        job.cancel();
        assert!(*rx.borrow());
    }
}
