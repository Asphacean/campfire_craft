//! D-07: the shape of a progress event, shared between the core and
//! `src-tauri`'s Tauri-channel adapter. The core takes a plain
//! `&dyn Fn(Progress)` sink (RESEARCH.md's "Claude's discretion" table) so
//! it stays Tauri-free and testable headlessly; only `src-tauri` wires this
//! to a real `tauri::ipc::Channel`. Manifest sync, Java/Forge provisioning
//! and launch (later plans in this phase) are the actual producers — this
//! task only lands the shared shape so those plans have somewhere to send
//! events without re-deciding the wire format.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum Progress {
    Step {
        label: String,
        current: u32,
        total: u32,
    },
    Done,
    Error {
        message: String,
    },
}

/// What a long-running operation is given to report through. `src-tauri`
/// adapts this to `tauri::ipc::Channel::send`; tests can pass a closure
/// that just records events into a `Vec`.
pub type ProgressSink<'a> = &'a dyn Fn(Progress);
