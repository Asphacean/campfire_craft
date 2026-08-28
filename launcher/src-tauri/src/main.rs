// Standard two-line shape Tauri's mobile-shaped bundling conventions expect
// (Phase 5) — all real logic lives in `lib.rs`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    campfire_launcher_lib::run();
}
