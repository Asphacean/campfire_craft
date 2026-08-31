//! The Tauri app builder and the commands the window calls — thin adapters
//! over `campfire_launcher_core`: parse, call, map the core's error variant
//! to a stable string the frontend switches on, log, return. The password
//! crosses this boundary as a command argument and is dropped at the end
//! of the call; nothing here stores it.
//!
//! D-18/LNCH-05: `play` and `verify_files` are the only two commands that
//! stream progress — both adapt `campfire_launcher_core`'s plain
//! `ProgressSink` closure to a `tauri::ipc::Channel`, never the general
//! event bus (04-RESEARCH.md's "Don't Hand-Roll" row).

use campfire_launcher_core::progress::Progress;
use campfire_launcher_core::{auth, log, manifest, play as play_core, status, strings, system, update};
use tauri::ipc::Channel;
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::UpdaterExt;

/// Returns this crate's own version, so `main.js` can prove the bridge
/// works by writing the result into the version footer on load.
#[tauri::command]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Every user-visible string, rendered once from `strings.rs` into
/// whichever language of code needs it (D-01).
#[tauri::command]
fn get_strings() -> serde_json::Value {
    strings::as_json()
}

#[tauri::command]
async fn get_status() -> status::ServerStatus {
    status::fetch_status().await
}

fn map_auth_error(e: auth::AuthError) -> String {
    match e {
        auth::AuthError::NickTaken => "nick_taken",
        auth::AuthError::InvalidNick => "invalid_nick",
        auth::AuthError::WeakPassword => "weak_password",
        auth::AuthError::InvalidCredentials => "invalid_credentials",
        auth::AuthError::InvalidToken => "invalid_token",
        auth::AuthError::RateLimited => "rate_limited",
        auth::AuthError::Network => "network",
        auth::AuthError::NoStoredSession => "no_stored_session",
    }
    .to_string()
}

#[derive(serde::Serialize)]
struct SessionView {
    nick: String,
    expires: i64,
}

impl From<auth::Session> for SessionView {
    fn from(s: auth::Session) -> Self {
        Self {
            nick: s.nick,
            expires: s.expires,
        }
    }
}

#[tauri::command]
async fn login(nick: String, password: String) -> Result<SessionView, String> {
    let (session, refresh_token) = auth::login(&nick, &password).await.map_err(map_auth_error)?;
    auth::store_refresh(&session.nick, &refresh_token).map_err(map_auth_error)?;
    Ok(session.into())
}

#[tauri::command]
async fn register(nick: String, password: String) -> Result<(), String> {
    auth::register(&nick, &password).await.map_err(map_auth_error)
}

/// Tried on load: if a stored refresh token is live, this returns straight
/// into the logged-in state with no password prompt (AUTH-03's real
/// proof). If the token is dead, the error string carries the attempted
/// nick after a `|` (`"invalid_token|SomeNick"`) so the frontend can show
/// the session-expired sentence with the nick pre-filled; a cold start
/// (nothing stored yet) is the bare code `no_stored_session` with no nick.
#[tauri::command]
async fn restore_session() -> Result<SessionView, String> {
    let (nick, refresh_token) = auth::load_refresh().ok_or("no_stored_session".to_string())?;
    match auth::refresh(&nick, &refresh_token).await {
        Ok((session, new_refresh)) => {
            auth::store_refresh(&session.nick, &new_refresh).map_err(map_auth_error)?;
            Ok(session.into())
        }
        Err(e) => Err(format!("{}|{nick}", map_auth_error(e))),
    }
}

#[tauri::command]
async fn logout(nick: String) {
    auth::logout(&nick).await;
}

/// Where `launcher.log` lives — still exposed for anything that just wants
/// the path (e.g. a future log-tail feature); the "Open log" button itself
/// now uses [`open_log`], which actually opens it.
#[tauri::command]
fn get_log_path() -> String {
    campfire_launcher_core::paths::log_path().to_string_lossy().to_string()
}

/// D-18: the play sequence over a channel, streaming the real step/byte
/// events `campfire_launcher_core::play::play` reports — never the event
/// bus (T-04-04's channel requirement). The RAM figure is clamped to the
/// slider's own 3..=10 range here, in Rust, before it ever reaches the
/// command builder (T-04-04-07) — the slider element's own `min`/`max`
/// attributes are a UI courtesy, not the enforcement.
#[derive(serde::Serialize)]
struct PlayErrorView {
    code: String,
    reopen_form: bool,
}

impl From<play_core::PlayError> for PlayErrorView {
    fn from(e: play_core::PlayError) -> Self {
        Self {
            code: e.code().to_string(),
            reopen_form: e.reopen_form(),
        }
    }
}

#[tauri::command]
async fn play(on_event: Channel<Progress>, nick: String, ram: f32) -> Result<(), PlayErrorView> {
    let ram = ram.clamp(3.0, 10.0);
    let sink = campfire_launcher_core::progress::sink_from(move |p: Progress| {
        let _ = on_event.send(p);
    });
    // Run the whole sequence on its own `tokio::spawn`ed task rather than
    // `.await`ing `play_core::play` directly in this command's own async
    // body: the deep chain it drives internally (`manifest::sync`'s
    // `buffer_unordered` download batch, in particular) tripped a known
    // rustc HRTB false-positive — "implementation of `FnOnce`/`Send` is
    // not general enough" — the moment it was reachable from
    // `tauri::generate_handler!`'s own command-dispatch macro. A spawned
    // task's `JoinHandle<T>` carries none of that internal type
    // complexity across the boundary the macro inspects; `nick`/`ram`/
    // `sink` are all owned, so the spawned future is genuinely `'static`.
    let joined = tokio::spawn(async move { play_core::play(&nick, ram, true, sink).await })
        .await
        .map_err(|_| PlayErrorView {
            code: "generic".to_string(),
            reopen_form: false,
        })?;
    joined.map(|_| ()).map_err(PlayErrorView::from)
}

/// "Verify files": the same core `manifest::verify` sync used, over the
/// same channel mechanism as `play`, returning the repaired count for the
/// informational (non-error) banner.
#[derive(serde::Serialize)]
struct VerifyReportView {
    checked: u32,
    repaired: u32,
}

#[tauri::command]
async fn verify_files(on_event: Channel<Progress>) -> Result<VerifyReportView, String> {
    let sink = campfire_launcher_core::progress::sink_from(move |p: Progress| {
        let _ = on_event.send(p);
    });
    manifest::verify(sink)
        .await
        .map(|r| VerifyReportView {
            checked: r.checked,
            repaired: r.repaired,
        })
        .map_err(|e| format!("{e:?}"))
}

/// "Game folder": reveals the install directory in the OS file manager.
#[tauri::command]
fn open_game_folder(app: tauri::AppHandle) -> Result<(), String> {
    app.opener()
        .reveal_item_in_dir(campfire_launcher_core::paths::game_dir())
        .map_err(|e| e.to_string())
}

/// "Open log": opens `launcher.log` itself with the OS default handler —
/// distinct from `open_game_folder`, which only reveals a directory.
#[tauri::command]
fn open_log(app: tauri::AppHandle) -> Result<(), String> {
    let path = campfire_launcher_core::paths::log_path().to_string_lossy().to_string();
    app.opener().open_path(path, None::<String>).map_err(|e| e.to_string())
}

/// D-06: the slider's own machine facts — total physical memory and the
/// formula's recommended default — computed in Rust, not guessed in JS.
#[derive(serde::Serialize)]
struct SystemMemoryView {
    total_gb: f32,
    recommended_gb: f32,
}

#[tauri::command]
fn system_memory() -> SystemMemoryView {
    let total = system::total_memory_gb();
    let recommended = system::recommended_ram_gb(total);
    SystemMemoryView {
        total_gb: total,
        recommended_gb: recommended,
    }
}

/// The version footer's other half: the `pack_version` the last
/// successful sync saw, cached to disk so it survives a restart even
/// before the first sync of this session completes.
#[tauri::command]
fn pack_version() -> Option<String> {
    manifest::cached_pack_version()
}

/// LNCH-08's startup check: fetches our own feed over the pinned CA and
/// compares semantically — never the plugin's own `updater().check()`,
/// which would hit the same feed a second time for no reason. A failed or
/// negative check is `None`; the frontend shows nothing at all in that
/// case (D-08: silent by contract).
#[derive(serde::Serialize)]
struct AvailableView {
    version: String,
    notes: String,
}

#[tauri::command]
async fn check_update() -> Option<AvailableView> {
    update::check(env!("CARGO_PKG_VERSION"))
        .await
        .map(|a| AvailableView {
            version: a.version,
            notes: a.notes,
        })
}

/// "Update now": the one path that actually replaces the running binary,
/// so it goes through `tauri-plugin-updater`'s own `Updater`/`Update` —
/// the only thing in this project that verifies the minisign signature —
/// rather than [`check_update`]'s plain version comparison. Re-fetches the
/// feed once more via the plugin's own `check()` to get the `Update`
/// handle `download_and_install` needs; [`check_update`]'s earlier fetch
/// only decided whether to show the dialog at all. Progress is forwarded
/// into the same channel shape `play`/`verify_files` already use, so the
/// window's one progress bar is reused rather than duplicated.
#[tauri::command]
async fn install_update(app: tauri::AppHandle, on_event: Channel<Progress>) -> Result<(), String> {
    let updater = app.updater().map_err(|e| {
        log::error(&format!("install-update: app.updater() failed: {e:?}"));
        "generic".to_string()
    })?;
    let update = updater
        .check()
        .await
        .map_err(|e| {
            log::error(&format!("install-update: plugin check() failed: {e:?}"));
            "generic".to_string()
        })?
        .ok_or_else(|| {
            log::error("install-update: plugin check() found no update (feed returned nothing newer)");
            "generic".to_string()
        })?;

    log::info(&format!("install-update: downloading {} -> {}", env!("CARGO_PKG_VERSION"), update.version));

    let start = std::time::Instant::now();
    let downloaded = std::sync::atomic::AtomicU64::new(0);
    let content_total = std::sync::Mutex::new(0u64);

    let result = update
        .download_and_install(
            |chunk_len, content_len| {
                let so_far = downloaded.fetch_add(chunk_len as u64, std::sync::atomic::Ordering::Relaxed)
                    + chunk_len as u64;
                if let Some(len) = content_len {
                    *content_total.lock().expect("content_total mutex poisoned") = len;
                }
                let total = *content_total.lock().expect("content_total mutex poisoned");
                let elapsed = start.elapsed().as_secs_f64().max(0.001);
                let _ = on_event.send(Progress::Bytes {
                    downloaded: so_far,
                    total,
                    per_sec: (so_far as f64 / elapsed) as u64,
                });
            },
            || {},
        )
        .await;

    match result {
        Ok(()) => {
            log::info(&format!(
                "install-update: download+install finished for {} in {:.1}s",
                update.version,
                start.elapsed().as_secs_f64()
            ));
            Ok(())
        }
        Err(e) => {
            // The full error chain (`{:?}`), not just `{e}` — a plugin TLS
            // failure or a signature mismatch usually only shows up in the
            // Debug source chain, and `install_update`'s only prior failure
            // mode was a silent "Something went wrong" with nothing after
            // it in launcher.log at all (this bug's whole observability
            // gap). The UI sentence itself stays the generic `errorGeneric`
            // string either way — only the log line carries detail.
            log::error(&format!("install-update: download_and_install failed: {e:?}"));
            Err("generic".to_string())
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(tauri::generate_handler![
            get_version,
            get_strings,
            get_status,
            login,
            register,
            restore_session,
            logout,
            get_log_path,
            play,
            verify_files,
            open_game_folder,
            open_log,
            system_memory,
            pack_version,
            check_update,
            install_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
