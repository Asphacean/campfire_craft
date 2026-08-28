//! D-07: the shape of a progress event, shared between the core and
//! `src-tauri`'s Tauri-channel adapter. The core takes a plain
//! `&dyn Fn(Progress)` sink (RESEARCH.md's "Claude's discretion" table) so
//! it stays Tauri-free and testable headlessly; only `src-tauri` wires this
//! to a real `tauri::ipc::Channel`.
//!
//! 04-02 is the first real producer (manifest sync/verify, Java fetch) and
//! settles the final shape: a `Step` for named progress out of a total, a
//! `Bytes` tick for a live transfer rate, `Done`, and a `Failed { code }`
//! carrying a stable machine-readable reason (never a formatted sentence —
//! `strings.rs` owns the user-facing copy, keyed off `code`).

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
/// onto a separate `'static` task) and, later, across `src-tauri`'s Tauri
/// channel adapter. `src-tauri` adapts this to `tauri::ipc::Channel::send`;
/// tests pass a closure that just records events into a `Vec`.
pub type ProgressSink<'a> = &'a (dyn Fn(Progress) + Send + Sync);

/// A sink that discards every event — for callers (mostly tests) that
/// don't care about progress reporting at all.
pub fn noop_sink(_: Progress) {}
