//! `campfire-launcher-core` — the Tauri-free half of the launcher. Every
//! filesystem, network, and process-spawn capability lives here so it can
//! be exercised headlessly (via `campfire-cli` and `cargo test`) without a
//! display. `src-tauri` is a thin adapter over this crate's public API.
//!
//! This plan (04-01) lands the session tracer: `http`, `paths`, `auth`,
//! `status`, `log`, `progress`, `strings`. 04-02 adds `manifest` (the
//! client pack sync) and `java` (per-platform Java 8 provisioning). 04-03
//! adds `mojang` (the vanilla bootstrap), `forge` (headless Forge install +
//! merge) and `launch` (the launch line: classpath, natives, offline UUID,
//! token handoff, seeded server list).

pub mod auth;
pub mod http;
pub mod java;
pub mod log;
pub mod manifest;
pub mod mojang;
pub mod paths;
pub mod progress;
pub mod status;
pub mod strings;
