//! The Tauri app builder and the commands the window calls — thin adapters
//! over `campfire_launcher_core`: parse, call, map the core's error variant
//! to a stable string the frontend switches on, log, return. The password
//! crosses this boundary as a command argument and is dropped at the end
//! of the call; nothing here stores it.

use campfire_launcher_core::{auth, status, strings};

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
fn logout(nick: String) {
    auth::logout(&nick);
}

/// Where `launcher.log` lives, so the "Open log" button has something to
/// show. Actually revealing it in the OS file manager is `tauri-plugin-
/// opener` territory — out of scope for this plan (no npm/new dependency
/// justified for one button in the tracer); wave 4, which also wires
/// "Game folder", is the natural place to add it.
#[tauri::command]
fn get_log_path() -> String {
    campfire_launcher_core::paths::log_path().to_string_lossy().to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_version,
            get_strings,
            get_status,
            login,
            register,
            restore_session,
            logout,
            get_log_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
