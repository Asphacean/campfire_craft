//! D-07: the shape of a progress event, shared between the core and
//! `src-tauri`'s Tauri-channel adapter. The core takes an owned, cheaply
//! cloneable callback handle (an `Arc`, not a borrowed reference) so it
//! stays Tauri-free and testable headlessly, while carrying no named
//! lifetime through the deep async call chain 04-04's Play sequence
//! introduces; only `src-tauri` wires this to a real
//! `tauri::ipc::Channel`.
//!
//! 04-02 settled the event shape: a `Step` for named progress out of a
//! total, a `Bytes` tick for a live transfer rate, `Done`, and a
//! `Failed { code }` carrying a stable machine-readable reason (never a
//! formatted sentence — `strings.rs` owns the user-facing copy).
//!
//! 04-04 changed [`ProgressSink`] from a borrowed `&'a (dyn Fn(Progress) +
//! Send + Sync)` to an owned `Arc<dyn Fn(Progress) + Send + Sync>`: the
//! borrowed form, reused across three nested async fns (`play` ->
//! `manifest::sync`/`mojang::ensure_vanilla`/`forge::ensure_forge`, each
//! itself closing over it inside a `buffer_unordered` stream combinator),
//! compiled standalone but failed the moment the same call chain was
//! wrapped by `tauri::generate_handler!`'s command dispatch — "error:
//! implementation of `FnOnce` is not general enough" — a known rustc HRTB
//! limitation with borrowed trait-object callbacks threaded through
//! nested async fns. An owned, `'static` `Arc` has no lifetime parameter
//! for that macro's generated wrapper to fail to generalize over; cloning
//! an `Arc` is a refcount bump, not a real copy.

use std::sync::Arc;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum Progress {
    Step { name: String, current: u32, total: u32 },
    Bytes { downloaded: u64, total: u64, per_sec: u64 },
    Done,
    Failed { code: String },
}

/// What a long-running operation is given to report through. `Send + Sync`
/// so a sink can be shared across the manifest sync's bounded-concurrent
/// downloads (polled within one task via `buffer_unordered`, never spawned
/// onto a separate `'static` task) and across `src-tauri`'s Tauri channel
/// adapter. `src-tauri` adapts this to `tauri::ipc::Channel::send`; tests
/// pass a closure that just records events into a `Vec`.
pub type ProgressSink = Arc<dyn Fn(Progress) + Send + Sync>;

/// Wraps any `Fn(Progress) + Send + Sync + 'static` closure or fn item as
/// a [`ProgressSink`] — the one place that spells `Arc::new` so call
/// sites read as intent ("make this a sink"), not as a smart-pointer
/// detail.
pub fn sink_from<F: Fn(Progress) + Send + Sync + 'static>(f: F) -> ProgressSink {
    Arc::new(f)
}

/// A sink that discards every event — for callers (mostly tests, and the
/// already-installed fast path of `campfire-cli launch-cmd`) that don't
/// care about progress reporting at all.
pub fn noop_sink() -> ProgressSink {
    Arc::new(|_: Progress| {})
}
