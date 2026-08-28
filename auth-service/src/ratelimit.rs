//! Hand-rolled per-IP sliding-window rate limiter. RESEARCH.md's own
//! "Alternatives Considered" table: a single endpoint at this traffic scale
//! (5-7 users, ever) does not justify learning `tower-governor`'s builder
//! API — a `Mutex<HashMap<IpAddr, Vec<Instant>>>` pruned on each check is
//! the lazier correct answer here.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct RateLimiter {
    window: Duration,
    limit: usize,
    hits: Mutex<HashMap<IpAddr, Vec<Instant>>>,
}

impl RateLimiter {
    pub fn new(window: Duration, limit: usize) -> Self {
        Self {
            window,
            limit,
            hits: Mutex::new(HashMap::new()),
        }
    }

    /// Records and allows this attempt if `ip` has fewer than `limit` hits
    /// within the trailing `window`; otherwise refuses without recording
    /// (an already-throttled caller doesn't get to push its own window
    /// further out by retrying). Used by `/register`, which counts every
    /// attempt (D-04).
    pub fn check(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut hits = self.hits.lock().expect("ratelimit mutex poisoned");
        let entry = hits.entry(ip).or_default();
        entry.retain(|t| now.duration_since(*t) < self.window);
        if entry.len() >= self.limit {
            false
        } else {
            entry.push(now);
            true
        }
    }

    /// Peek whether `ip` is currently under the limit, without recording an
    /// attempt. Used by `/login`, which only counts *failures* (recorded
    /// separately via [`Self::record_failure`]) — a successful login must
    /// never be throttled by prior unrelated failures, and checking must
    /// not itself consume quota.
    pub fn would_allow(&self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let mut hits = self.hits.lock().expect("ratelimit mutex poisoned");
        let entry = hits.entry(ip).or_default();
        entry.retain(|t| now.duration_since(*t) < self.window);
        entry.len() < self.limit
    }

    /// Record a failed attempt for `ip` (pruning stale entries first).
    pub fn record_failure(&self, ip: IpAddr) {
        let now = Instant::now();
        let mut hits = self.hits.lock().expect("ratelimit mutex poisoned");
        let entry = hits.entry(ip).or_default();
        entry.retain(|t| now.duration_since(*t) < self.window);
        entry.push(now);
    }
}
