// Standard two-line shape Tauri's mobile-shaped bundling conventions expect
// (Phase 5) — all real logic lives in `lib.rs`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

/// `--help`/`-h` exits before the Tauri builder ever tries to open a
/// window — this is what makes `cargo tauri build --no-bundle`'s own
/// binary verifiable on a host with no display (this Pi): every other
/// invocation opens the real window, exactly as a friend's machine would.
fn main() {
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        println!("campfire-launcher {}", env!("CARGO_PKG_VERSION"));
        println!("The campfire.pub desktop launcher. Run with no arguments to open the window.");
        return;
    }
    campfire_launcher_lib::run();
}
