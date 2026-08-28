//! `campfire-launcher-core` — the Tauri-free half of the launcher. Every
//! filesystem, network, and process-spawn capability lives here so it can
//! be exercised headlessly (via `campfire-cli` and `cargo test`) without a
//! display. `src-tauri` is a thin adapter over this crate's public API.
//!
//! Filled in across this phase's plans: task 1 (this plan) proves the
//! workspace builds; task 3 (this plan) adds `http`, `paths`, `auth`,
//! `status`, `log`, `progress`, `strings`. Later plans add manifest sync,
//! Java/Forge provisioning, and launch.
