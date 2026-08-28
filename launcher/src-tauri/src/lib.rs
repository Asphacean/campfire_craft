//! The Tauri app builder and the commands the window calls. Task 3 (this
//! plan) adds the real auth/status bridge; this task registers one command
//! that proves `window.__TAURI__.core.invoke` reaches Rust at all.

/// Returns this crate's own version, so `main.js` can prove the bridge
/// works by writing the result into the version footer on load.
#[tauri::command]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_version])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
